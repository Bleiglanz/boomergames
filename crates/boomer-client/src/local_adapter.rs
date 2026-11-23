//! Offline Step 1 transport adapter running game sessions locally in WASM.

use std::collections::VecDeque;

use boomer_domain::checkers::{CheckersColor, CheckersGame, CheckersSearch, SearchProgress};
use boomer_domain::rng::SplitMix64;
use boomer_domain::{BotLevel, GameKind, LocalMode, Seat};
use boomer_protocol::{
    CheckersClockModeDto, CheckersSettingsDto, CheckersStateDto, ClientMessage, ErrorCode,
    PROTOCOL_VERSION, ServerMessage, SessionKindDto,
};

use crate::mapping::{
    checkers_color_to_dto, checkers_move_to_dto, checkers_outcome_to_dto, game_kind_from_dto,
    game_kind_to_dto, local_mode_from_dto, local_mode_to_dto, notation_path_to_squares,
    seat_to_dto,
};
use crate::ports::{GameTransport, TransportError};

/// Node budget per `tick(dt_ms)` frame for time-sliced Checkers AI (~2-4 ms in WASM).
const CHECKERS_AI_NODES_PER_TICK: u32 = 4_000;

/// Minimum natural delay (ms) before the bot displays its move so turns feel readable.
const BOT_MIN_THINK_MS: u32 = 350;

/// Active offline Checkers match managed by [`LocalSessionAdapter`].
#[derive(Debug)]
struct LocalCheckersSession {
    game: CheckersGame,
    mode: LocalMode,
    settings: CheckersSettingsDto,
    seat_colors: [CheckersColor; 2],
    turn_number: u32,
    clocks_ms: [u32; 2],
    turn_ms_left: u32,
    clock_emit_accum_ms: u32,
    draw_offered_by: Option<Seat>,
    bot_search: Option<CheckersSearch>,
    bot_pending_move: Option<Vec<boomer_domain::checkers::Square>>,
    bot_think_ms: u32,
}

impl LocalCheckersSession {
    fn new(
        mode: LocalMode,
        settings: CheckersSettingsDto,
        seat_colors: [CheckersColor; 2],
        rng: &mut SplitMix64,
    ) -> Self {
        let initial_game_ms = settings.game_minutes.saturating_mul(60_000);
        let initial_move_ms = settings.move_timer_secs.saturating_mul(1_000);
        let mut session = Self {
            game: CheckersGame::new(),
            mode,
            settings,
            seat_colors,
            turn_number: 1,
            clocks_ms: [initial_game_ms, initial_game_ms],
            turn_ms_left: initial_move_ms,
            clock_emit_accum_ms: 0,
            draw_offered_by: None,
            bot_search: None,
            bot_pending_move: None,
            bot_think_ms: 0,
        };
        session.maybe_start_bot_search(rng);
        session
    }

    const fn seat_for_color(&self, color: CheckersColor) -> Seat {
        let [seat_one, _] = self.seat_colors;
        if seat_one as u8 == color as u8 {
            Seat::One
        } else {
            Seat::Two
        }
    }

    const fn active_seat(&self) -> Seat {
        self.seat_for_color(self.game.board().turn)
    }

    const fn bot_level(&self) -> Option<BotLevel> {
        match self.mode {
            LocalMode::VsBot(level) => Some(level),
            LocalMode::Hotseat | LocalMode::FreePractice => None,
        }
    }

    const fn is_bot_turn(&self) -> bool {
        self.bot_level().is_some() && matches!(self.active_seat(), Seat::Two)
    }

    fn maybe_start_bot_search(&mut self, rng: &mut SplitMix64) {
        self.bot_search = None;
        self.bot_pending_move = None;
        self.bot_think_ms = 0;

        if self.game.outcome().is_some() {
            return;
        }
        if let Some(level) = self.bot_level()
            && self.is_bot_turn()
        {
            let seed = rng.next_u64();
            self.bot_search = Some(CheckersSearch::new(self.game.board(), level, seed));
        }
    }

    fn reset_move_timer(&mut self) {
        self.turn_ms_left = self.settings.move_timer_secs.saturating_mul(1_000);
        self.clock_emit_accum_ms = 0;
    }

    fn to_snapshot_dto(&self) -> CheckersStateDto {
        let [s1, s2] = self.seat_colors;
        let active_seat = self.active_seat();
        let bot_thinking = self.is_bot_turn() && self.game.outcome().is_none();
        let legal_moves = if bot_thinking || self.game.outcome().is_some() {
            Vec::new()
        } else {
            self.game
                .legal_moves()
                .iter()
                .map(checkers_move_to_dto)
                .collect()
        };
        let (captured_by_black, captured_by_white) = self.game.captured_counts();
        let last_move = self
            .game
            .last_move()
            .map(|mv| mv.path.iter().map(|sq| sq.notation()).collect());

        CheckersStateDto {
            board: self.game.board().to_board_string(),
            turn: checkers_color_to_dto(self.game.board().turn),
            active_seat: seat_to_dto(active_seat),
            seat_colors: [checkers_color_to_dto(s1), checkers_color_to_dto(s2)],
            turn_number: self.turn_number,
            moves: self.game.move_history().to_vec(),
            last_move,
            mandatory_capture: !bot_thinking && self.game.has_mandatory_capture(),
            legal_moves,
            captured_by_black,
            captured_by_white,
            settings: self.settings,
            clocks_ms: self.clocks_ms,
            turn_ms_left: self.turn_ms_left,
            draw_offered_by: self.draw_offered_by.map(seat_to_dto),
            bot_thinking,
            outcome: self
                .game
                .outcome()
                .map(|out| checkers_outcome_to_dto(out, self.seat_colors)),
        }
    }
}

/// Offline [`GameTransport`] adapter for single-player Hotseat, vs. AI, and Free Practice.
#[derive(Debug)]
pub struct LocalSessionAdapter {
    handshaken: bool,
    active_game: Option<GameKind>,
    active_mode: Option<LocalMode>,
    checkers_settings: CheckersSettingsDto,
    checkers: Option<LocalCheckersSession>,
    rng: SplitMix64,
    outbox: VecDeque<ServerMessage>,
}

impl Default for LocalSessionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalSessionAdapter {
    /// Creates a new offline session adapter with a default deterministic seed.
    #[must_use]
    pub fn new() -> Self {
        Self::with_seed(0xB00E_CAFE_1234_5678)
    }

    /// Creates a new offline session adapter with an explicit deterministic seed.
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        Self {
            handshaken: false,
            active_game: None,
            active_mode: None,
            checkers_settings: CheckersSettingsDto::default(),
            checkers: None,
            rng: SplitMix64::new(seed),
            outbox: VecDeque::new(),
        }
    }

    fn emit_checkers_snapshot(&mut self) {
        if let Some(session) = &self.checkers {
            self.outbox.push_back(ServerMessage::CheckersSnapshot {
                state: session.to_snapshot_dto(),
            });
        }
    }

    fn push_error(&mut self, code: ErrorCode, re: &str) {
        self.outbox.push_back(ServerMessage::Error {
            code,
            re: re.to_owned(),
        });
    }

    fn handle_checkers_move(&mut self, turn: u32, path: &[u8]) {
        let Some(session) = self.checkers.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "checkers.move");
            return;
        };
        if session.game.outcome().is_some() {
            self.push_error(ErrorCode::NotAllowed, "checkers.move");
            return;
        }
        if session.is_bot_turn() {
            self.push_error(ErrorCode::NotYourTurn, "checkers.move");
            return;
        }
        if turn != session.turn_number {
            self.push_error(ErrorCode::StaleTurn, "checkers.move");
            return;
        }
        let Some(squares) = notation_path_to_squares(path) else {
            self.push_error(ErrorCode::Invalid, "checkers.move");
            return;
        };
        if session.game.apply_path(&squares).is_err() {
            self.push_error(ErrorCode::IllegalAction, "checkers.move");
            return;
        }
        session.turn_number = session.turn_number.saturating_add(1);
        session.draw_offered_by = None;
        session.reset_move_timer();
        session.maybe_start_bot_search(&mut self.rng);
        self.emit_checkers_snapshot();
    }

    fn handle_checkers_settings(&mut self, settings: CheckersSettingsDto) {
        self.checkers_settings = settings;
        if let Some(existing) = &self.checkers {
            let mode = existing.mode;
            let colors = existing.seat_colors;
            self.checkers = Some(LocalCheckersSession::new(
                mode,
                self.checkers_settings,
                colors,
                &mut self.rng,
            ));
            self.emit_checkers_snapshot();
        }
    }

    fn handle_resign(&mut self) {
        let Some(session) = self.checkers.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "resign");
            return;
        };
        let loser_color = if session.bot_level().is_some() {
            // In vs. Bot mode, the human is always Seat One.
            let [seat_one_color, _] = session.seat_colors;
            seat_one_color
        } else {
            session.game.board().turn
        };
        if session.game.resign(loser_color).is_err() {
            self.push_error(ErrorCode::NotAllowed, "resign");
            return;
        }
        session.bot_search = None;
        session.bot_pending_move = None;
        session.draw_offered_by = None;
        self.emit_checkers_snapshot();
    }

    fn handle_draw_offer(&mut self) {
        let Some(session) = self.checkers.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "draw.offer");
            return;
        };
        if session.game.outcome().is_some() {
            self.push_error(ErrorCode::NotAllowed, "draw.offer");
            return;
        }
        if session.bot_level().is_some() {
            // Bot accepts draw only if it is down material or in a quiet equal endgame.
            let (black_cnt, white_cnt) = session.game.board().piece_counts();
            let [_, bot_color] = session.seat_colors;
            let (bot_cnt, human_cnt) = match bot_color {
                CheckersColor::Black => (black_cnt, white_cnt),
                CheckersColor::White => (white_cnt, black_cnt),
            };
            let accept = bot_cnt < human_cnt || (bot_cnt == human_cnt && bot_cnt <= 3);
            if accept {
                let _ = session.game.agree_draw();
                session.bot_search = None;
                session.bot_pending_move = None;
                session.draw_offered_by = None;
                self.emit_checkers_snapshot();
            } else {
                session.draw_offered_by = None;
                self.emit_checkers_snapshot();
                self.push_error(ErrorCode::NotAllowed, "draw.offer");
            }
        } else {
            session.draw_offered_by = Some(session.active_seat());
            self.emit_checkers_snapshot();
        }
    }

    fn handle_draw_answer(&mut self, accept: bool) {
        let Some(session) = self.checkers.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "draw.answer");
            return;
        };
        if session.draw_offered_by.is_none() || session.game.outcome().is_some() {
            self.push_error(ErrorCode::NotAllowed, "draw.answer");
            return;
        }
        session.draw_offered_by = None;
        if accept {
            let _ = session.game.agree_draw();
        }
        self.emit_checkers_snapshot();
    }

    fn handle_rematch(&mut self) {
        let Some(existing) = &self.checkers else {
            self.push_error(ErrorCode::NotAllowed, "rematch");
            return;
        };
        let [s1, s2] = existing.seat_colors;
        // Alternate starting colors on every rematch per plan.md C-1.
        let swapped = [s2, s1];
        let mode = existing.mode;
        let settings = existing.settings;
        self.checkers = Some(LocalCheckersSession::new(
            mode,
            settings,
            swapped,
            &mut self.rng,
        ));
        self.emit_checkers_snapshot();
    }
}

impl GameTransport for LocalSessionAdapter {
    fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        match msg {
            ClientMessage::Hello { v, .. } => {
                if v != PROTOCOL_VERSION {
                    self.push_error(ErrorCode::UpgradeRequired, "hello");
                    return Ok(());
                }
                self.handshaken = true;
                self.outbox.push_back(ServerMessage::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    mode: SessionKindDto::Local,
                });
                self.outbox.push_back(ServerMessage::SessionState {
                    game: None,
                    mode: None,
                    status: "home_ready".to_owned(),
                });
                Ok(())
            }
            ClientMessage::LocalStart { game, mode } => {
                if !self.handshaken {
                    self.push_error(ErrorCode::NotAllowed, "local.start");
                    return Ok(());
                }
                let domain_game = game_kind_from_dto(game);
                let domain_mode = local_mode_from_dto(mode);
                if domain_game == GameKind::Checkers && domain_mode == LocalMode::FreePractice {
                    self.push_error(ErrorCode::Invalid, "local.start");
                    return Ok(());
                }
                self.active_game = Some(domain_game);
                self.active_mode = Some(domain_mode);
                let status = match domain_game {
                    GameKind::Checkers => {
                        self.checkers = Some(LocalCheckersSession::new(
                            domain_mode,
                            self.checkers_settings,
                            [CheckersColor::Black, CheckersColor::White],
                            &mut self.rng,
                        ));
                        "checkers_active"
                    }
                    GameKind::Pool => {
                        self.checkers = None;
                        "pool_shell_ready"
                    }
                };
                self.outbox.push_back(ServerMessage::SessionState {
                    game: Some(game_kind_to_dto(domain_game)),
                    mode: Some(local_mode_to_dto(domain_mode)),
                    status: status.to_owned(),
                });
                if domain_game == GameKind::Checkers {
                    self.emit_checkers_snapshot();
                }
                Ok(())
            }
            ClientMessage::LocalLeave {} => {
                self.active_game = None;
                self.active_mode = None;
                self.checkers = None;
                self.outbox.push_back(ServerMessage::SessionState {
                    game: None,
                    mode: None,
                    status: "home_ready".to_owned(),
                });
                Ok(())
            }
            ClientMessage::CheckersMove { turn, path } => {
                self.handle_checkers_move(turn, &path);
                Ok(())
            }
            ClientMessage::CheckersSettings { settings } => {
                self.handle_checkers_settings(settings);
                Ok(())
            }
            ClientMessage::Resign {} => {
                self.handle_resign();
                Ok(())
            }
            ClientMessage::DrawOffer {} => {
                self.handle_draw_offer();
                Ok(())
            }
            ClientMessage::DrawAnswer { accept } => {
                self.handle_draw_answer(accept);
                Ok(())
            }
            ClientMessage::Rematch {} => {
                self.handle_rematch();
                Ok(())
            }
        }
    }

    fn poll_messages(&mut self) -> Vec<ServerMessage> {
        self.outbox.drain(..).collect()
    }

    fn tick(&mut self, dt_ms: u32) {
        let mut needs_snapshot = false;

        if let Some(session) = self.checkers.as_mut()
            && session.game.outcome().is_none()
        {
            // 1. Advance active player's clock.
            if !session.is_bot_turn() {
                match session.settings.clock_mode {
                    CheckersClockModeDto::Unlimited => {}
                    CheckersClockModeDto::Move => {
                        session.turn_ms_left = session.turn_ms_left.saturating_sub(dt_ms);
                        session.clock_emit_accum_ms =
                            session.clock_emit_accum_ms.saturating_add(dt_ms);
                        if session.turn_ms_left == 0 {
                            let active_color = session.game.board().turn;
                            let _ = session.game.timeout(active_color);
                            needs_snapshot = true;
                        } else if session.clock_emit_accum_ms >= 250 {
                            session.clock_emit_accum_ms = 0;
                            needs_snapshot = true;
                        }
                    }
                    CheckersClockModeDto::Game => {
                        let active_seat = session.active_seat();
                        let [c1, c2] = &mut session.clocks_ms;
                        let active_clock = match active_seat {
                            Seat::One => c1,
                            Seat::Two => c2,
                        };
                        *active_clock = active_clock.saturating_sub(dt_ms);
                        session.clock_emit_accum_ms =
                            session.clock_emit_accum_ms.saturating_add(dt_ms);
                        if *active_clock == 0 {
                            let active_color = session.game.board().turn;
                            let _ = session.game.timeout(active_color);
                            needs_snapshot = true;
                        } else if session.clock_emit_accum_ms >= 250 {
                            session.clock_emit_accum_ms = 0;
                            needs_snapshot = true;
                        }
                    }
                }
            }

            // 2. Advance time-sliced Checkers AI search when it is the bot's turn.
            if session.game.outcome().is_none() && session.is_bot_turn() {
                session.bot_think_ms = session.bot_think_ms.saturating_add(dt_ms);
                if session.bot_pending_move.is_none()
                    && let Some(search) = session.bot_search.as_mut()
                {
                    match search.step(CHECKERS_AI_NODES_PER_TICK) {
                        SearchProgress::InProgress => {}
                        SearchProgress::Done(maybe_move) => {
                            session.bot_search = None;
                            session.bot_pending_move = maybe_move.map(|mv| mv.path);
                        }
                    }
                }

                if session.bot_think_ms >= BOT_MIN_THINK_MS
                    && let Some(path) = session.bot_pending_move.take()
                    && session.game.apply_path(&path).is_ok()
                {
                    session.turn_number = session.turn_number.saturating_add(1);
                    session.draw_offered_by = None;
                    session.reset_move_timer();
                    needs_snapshot = true;
                }
            }
        }

        if needs_snapshot {
            self.emit_checkers_snapshot();
        }
    }
}
