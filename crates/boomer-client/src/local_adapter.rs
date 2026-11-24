//! Offline Step 1 transport adapter running Checkers & 8-Ball Pool locally in WASM.

use std::collections::VecDeque;

use boomer_domain::checkers::{CheckersColor, CheckersGame, CheckersSearch, SearchProgress};
use boomer_domain::math::Vec2;
use boomer_domain::pool::{
    PoolFoul, PoolGame, PoolSearch, PoolSearchProgress, PoolSettings, ShotInput,
};
use boomer_domain::rng::SplitMix64;
use boomer_domain::{BotLevel, GameKind, LocalMode, Seat};
use boomer_protocol::{
    CheckersClockModeDto, CheckersSettingsDto, CheckersStateDto, ClientMessage, ErrorCode,
    PROTOCOL_VERSION, PoolSettingsDto, PoolShotDto, PoolStateDto, ServerMessage, SessionKindDto,
};

use crate::mapping::{
    ball_group_to_dto, checkers_color_to_dto, checkers_move_to_dto, checkers_outcome_to_dto,
    game_kind_from_dto, game_kind_to_dto, local_mode_from_dto, local_mode_to_dto,
    notation_path_to_squares, pool_balls_to_dto, pool_foul_to_dto, pool_outcome_to_dto,
    pool_settings_from_dto, pool_settings_to_dto, pool_shot_from_dto, pool_shot_to_dto,
    seat_to_dto,
};
use crate::ports::{GameTransport, TransportError};

/// Node budget per `tick(dt_ms)` frame for time-sliced Checkers AI (~2-4 ms in WASM).
const CHECKERS_AI_NODES_PER_TICK: u32 = 4_000;

/// Physics rollout budget per `tick(dt_ms)` frame for time-sliced Pool AI (~5-7 ms in WASM).
const POOL_AI_ROLLOUTS_PER_TICK: u32 = 3;

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

/// Active offline 8-Ball Pool match managed by [`LocalSessionAdapter`].
#[derive(Debug)]
struct LocalPoolSession {
    game: PoolGame,
    mode: LocalMode,
    breaker: Seat,
    turn_ms_left: u32,
    clock_emit_accum_ms: u32,
    anim_lock_ms: u32,
    last_fouls: Vec<PoolFoul>,
    last_potted: Vec<u8>,
    reracked: bool,
    bot_search: Option<PoolSearch>,
    bot_pending_shot: Option<ShotInput>,
    bot_think_ms: u32,
}

impl LocalPoolSession {
    fn new(mode: LocalMode, settings: PoolSettings, breaker: Seat, rng: &mut SplitMix64) -> Self {
        let free_practice = mode == LocalMode::FreePractice;
        let seed = rng.next_u64();
        let game = PoolGame::new(breaker, settings, free_practice, seed);
        let turn_ms_left = settings.shot_timer_secs.saturating_mul(1_000);
        let mut session = Self {
            game,
            mode,
            breaker,
            turn_ms_left,
            clock_emit_accum_ms: 0,
            anim_lock_ms: 0,
            last_fouls: Vec::new(),
            last_potted: Vec::new(),
            reracked: false,
            bot_search: None,
            bot_pending_shot: None,
            bot_think_ms: 0,
        };
        session.maybe_start_bot_search(rng);
        session
    }

    const fn bot_level(&self) -> Option<BotLevel> {
        match self.mode {
            LocalMode::VsBot(level) => Some(level),
            LocalMode::Hotseat | LocalMode::FreePractice => None,
        }
    }

    const fn is_bot_turn(&self) -> bool {
        self.bot_level().is_some() && matches!(self.game.active_seat(), Seat::Two)
    }

    fn reset_shot_timer(&mut self) {
        self.turn_ms_left = self.game.settings().shot_timer_secs.saturating_mul(1_000);
        self.clock_emit_accum_ms = 0;
    }

    fn maybe_start_bot_search(&mut self, rng: &mut SplitMix64) {
        self.bot_search = None;
        self.bot_pending_shot = None;
        self.bot_think_ms = 0;
        if self.game.outcome().is_some() {
            return;
        }
        if let Some(level) = self.bot_level()
            && self.is_bot_turn()
        {
            let seed = rng.next_u64();
            self.bot_search = Some(PoolSearch::new(&self.game, level, seed));
        }
    }

    fn to_snapshot_dto(&self) -> PoolStateDto {
        let [g1, g2] = self.game.groups();
        let [p1, p2] = self.game.pocketed_by_seat();
        let active_seat = self.game.active_seat();
        let bot_thinking = self.is_bot_turn() && self.game.outcome().is_none();

        PoolStateDto {
            balls: pool_balls_to_dto(self.game.balls()),
            active_seat: seat_to_dto(active_seat),
            groups: [g1.map(ball_group_to_dto), g2.map(ball_group_to_dto)],
            pocketed_by_seat: [p1.clone(), p2.clone()],
            is_break_shot: self.game.is_break_shot(),
            ball_in_hand: self.game.ball_in_hand(),
            ball_in_hand_behind_line: self.game.ball_in_hand_behind_line(),
            is_on_eight_ball: self.game.is_on_eight_ball(active_seat),
            free_practice: self.game.is_free_practice(),
            turn_number: self.game.turn_number(),
            settings: pool_settings_to_dto(self.game.settings()),
            turn_ms_left: self.turn_ms_left,
            last_fouls: self
                .last_fouls
                .iter()
                .copied()
                .map(pool_foul_to_dto)
                .collect(),
            last_potted: self.last_potted.clone(),
            reracked: self.reracked,
            bot_thinking,
            outcome: self.game.outcome().map(pool_outcome_to_dto),
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
    pool_settings: PoolSettings,
    checkers: Option<LocalCheckersSession>,
    pool: Option<LocalPoolSession>,
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
            pool_settings: PoolSettings::default(),
            checkers: None,
            pool: None,
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

    fn emit_pool_snapshot(&mut self) {
        if let Some(session) = &self.pool {
            self.outbox.push_back(ServerMessage::PoolSnapshot {
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

    fn handle_pool_shoot(&mut self, shot_dto: PoolShotDto) {
        let Some(session) = self.pool.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "pool.shoot");
            return;
        };
        if session.game.outcome().is_some() || session.anim_lock_ms > 0 {
            self.push_error(ErrorCode::NotAllowed, "pool.shoot");
            return;
        }
        if session.is_bot_turn() {
            self.push_error(ErrorCode::NotYourTurn, "pool.shoot");
            return;
        }
        if shot_dto.turn != session.game.turn_number() {
            self.push_error(ErrorCode::StaleTurn, "pool.shoot");
            return;
        }

        let pre_balls = pool_balls_to_dto(session.game.balls());
        let shot_input = pool_shot_from_dto(shot_dto);
        let Ok(outcome) = session.game.shoot(shot_input, false) else {
            self.push_error(ErrorCode::IllegalAction, "pool.shoot");
            return;
        };

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "duration_ms is clamped non-negative simulation duration (<= 18_000 ms)"
        )]
        let dur_ms = outcome.simulation.duration_ms.clamp(0.0, 25_000.0) as u32;
        session.anim_lock_ms = dur_ms;
        session.last_fouls = outcome.fouls;
        session.last_potted = outcome.potted.iter().map(|&(ball, _)| ball).collect();
        session.reracked = outcome.reracked;
        session.reset_shot_timer();
        session.maybe_start_bot_search(&mut self.rng);

        let state = session.to_snapshot_dto();
        self.outbox.push_back(ServerMessage::PoolShot {
            pre_balls,
            shot: shot_dto,
            state,
        });
    }

    fn handle_pool_place_cue(&mut self, x: f64, y: f64) {
        let Some(session) = self.pool.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "pool.place_cue");
            return;
        };
        if session.is_bot_turn() || session.anim_lock_ms > 0 {
            self.push_error(ErrorCode::NotYourTurn, "pool.place_cue");
            return;
        }
        if session.game.place_cue_ball(Vec2::new(x, y)).is_err() {
            self.push_error(ErrorCode::IllegalAction, "pool.place_cue");
            return;
        }
        self.emit_pool_snapshot();
    }

    fn handle_pool_settings(&mut self, settings: PoolSettingsDto) {
        self.pool_settings = pool_settings_from_dto(settings);
        if let Some(session) = self.pool.as_mut() {
            session.game.set_settings(self.pool_settings);
            session.reset_shot_timer();
            self.emit_pool_snapshot();
        }
    }

    fn handle_pool_rerack(&mut self) {
        let Some(session) = self.pool.as_mut() else {
            self.push_error(ErrorCode::NotAllowed, "pool.rerack");
            return;
        };
        if !session.game.is_free_practice() {
            self.push_error(ErrorCode::NotAllowed, "pool.rerack");
            return;
        }
        session.game.rerack(Seat::One);
        session.anim_lock_ms = 0;
        session.last_fouls.clear();
        session.last_potted.clear();
        session.reracked = true;
        self.emit_pool_snapshot();
    }

    fn handle_resign(&mut self) {
        if let Some(session) = self.checkers.as_mut() {
            let loser_color = if session.bot_level().is_some() {
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
            return;
        }

        if let Some(session) = self.pool.as_mut() {
            if session.game.is_free_practice() {
                self.push_error(ErrorCode::NotAllowed, "resign");
                return;
            }
            let loser = if session.bot_level().is_some() {
                Seat::One
            } else {
                session.game.active_seat()
            };
            if session.game.resign(loser).is_err() {
                self.push_error(ErrorCode::NotAllowed, "resign");
                return;
            }
            session.bot_search = None;
            session.bot_pending_shot = None;
            self.emit_pool_snapshot();
            return;
        }

        self.push_error(ErrorCode::NotAllowed, "resign");
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
        if let Some(existing) = &self.checkers {
            let [s1, s2] = existing.seat_colors;
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
            return;
        }
        if let Some(existing) = &self.pool {
            let mode = existing.mode;
            let settings = existing.game.settings();
            let next_breaker = if mode == LocalMode::FreePractice {
                Seat::One
            } else {
                existing.breaker.opponent()
            };
            self.pool = Some(LocalPoolSession::new(
                mode,
                settings,
                next_breaker,
                &mut self.rng,
            ));
            self.emit_pool_snapshot();
            return;
        }
        self.push_error(ErrorCode::NotAllowed, "rematch");
    }

    fn tick_checkers(&mut self, dt_ms: u32) {
        let mut needs_snapshot = false;

        if let Some(session) = self.checkers.as_mut()
            && session.game.outcome().is_none()
        {
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

    fn tick_pool(&mut self, dt_ms: u32) {
        let mut emitted_shot: Option<ServerMessage> = None;
        let mut needs_snapshot = false;

        if let Some(session) = self.pool.as_mut()
            && session.game.outcome().is_none()
        {
            // Count down previous shot animation window before starting shot clock / firing bot shot.
            if session.anim_lock_ms > 0 {
                session.anim_lock_ms = session.anim_lock_ms.saturating_sub(dt_ms);
            }

            // Advance time-sliced Pool AI rollouts in the background even during animation!
            if session.is_bot_turn()
                && session.bot_pending_shot.is_none()
                && let Some(search) = session.bot_search.as_mut()
            {
                match search.step(POOL_AI_ROLLOUTS_PER_TICK) {
                    PoolSearchProgress::InProgress => {}
                    PoolSearchProgress::Done(shot) => {
                        session.bot_search = None;
                        session.bot_pending_shot = Some(shot);
                    }
                }
            }

            if session.anim_lock_ms == 0 {
                if session.is_bot_turn() {
                    session.bot_think_ms = session.bot_think_ms.saturating_add(dt_ms);
                    if session.bot_think_ms >= BOT_MIN_THINK_MS
                        && let Some(shot_input) = session.bot_pending_shot.take()
                    {
                        let turn = session.game.turn_number();
                        let pre_balls = pool_balls_to_dto(session.game.balls());
                        if let Ok(outcome) = session.game.shoot(shot_input, false) {
                            #[expect(
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss,
                                reason = "clamped non-negative duration_ms <= 25_000"
                            )]
                            let dur_ms = outcome.simulation.duration_ms.clamp(0.0, 25_000.0) as u32;
                            session.anim_lock_ms = dur_ms;
                            session.last_fouls = outcome.fouls;
                            session.last_potted =
                                outcome.potted.iter().map(|&(ball, _)| ball).collect();
                            session.reracked = outcome.reracked;
                            session.reset_shot_timer();
                            session.maybe_start_bot_search(&mut self.rng);
                            emitted_shot = Some(ServerMessage::PoolShot {
                                pre_balls,
                                shot: pool_shot_to_dto(turn, shot_input),
                                state: session.to_snapshot_dto(),
                            });
                        }
                    }
                } else if !session.game.is_free_practice()
                    && session.game.settings().shot_timer_secs > 0
                {
                    session.turn_ms_left = session.turn_ms_left.saturating_sub(dt_ms);
                    session.clock_emit_accum_ms = session.clock_emit_accum_ms.saturating_add(dt_ms);
                    if session.turn_ms_left == 0 {
                        let _ = session.game.timeout_turn();
                        session.last_fouls.clear();
                        session.last_potted.clear();
                        session.reracked = false;
                        session.reset_shot_timer();
                        session.maybe_start_bot_search(&mut self.rng);
                        needs_snapshot = true;
                    } else if session.clock_emit_accum_ms >= 250 {
                        session.clock_emit_accum_ms = 0;
                        needs_snapshot = true;
                    }
                }
            }
        }

        if let Some(msg) = emitted_shot {
            self.outbox.push_back(msg);
        } else if needs_snapshot {
            self.emit_pool_snapshot();
        }
    }
    fn handle_local_start(
        &mut self,
        game: boomer_protocol::GameKindDto,
        mode: boomer_protocol::LocalModeDto,
    ) {
        if !self.handshaken {
            self.push_error(ErrorCode::NotAllowed, "local.start");
            return;
        }
        let domain_game = game_kind_from_dto(game);
        let domain_mode = local_mode_from_dto(mode);
        if domain_game == GameKind::Checkers && domain_mode == LocalMode::FreePractice {
            self.push_error(ErrorCode::Invalid, "local.start");
            return;
        }
        self.active_game = Some(domain_game);
        self.active_mode = Some(domain_mode);
        let status = match domain_game {
            GameKind::Checkers => {
                self.pool = None;
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
                self.pool = Some(LocalPoolSession::new(
                    domain_mode,
                    self.pool_settings,
                    Seat::One,
                    &mut self.rng,
                ));
                "pool_active"
            }
        };
        self.outbox.push_back(ServerMessage::SessionState {
            game: Some(game_kind_to_dto(domain_game)),
            mode: Some(local_mode_to_dto(domain_mode)),
            status: status.to_owned(),
        });
        match domain_game {
            GameKind::Checkers => self.emit_checkers_snapshot(),
            GameKind::Pool => self.emit_pool_snapshot(),
        }
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
                    me: None,
                    guest: None,
                    rooms: Vec::new(),
                });
                self.outbox.push_back(ServerMessage::SessionState {
                    game: None,
                    mode: None,
                    status: "home_ready".to_owned(),
                });
                Ok(())
            }
            ClientMessage::LocalStart { game, mode } => {
                self.handle_local_start(game, mode);
                Ok(())
            }
            ClientMessage::LocalLeave {} => {
                self.active_game = None;
                self.active_mode = None;
                self.checkers = None;
                self.pool = None;
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
            ClientMessage::PoolShoot { shot } => {
                self.handle_pool_shoot(shot);
                Ok(())
            }
            ClientMessage::PoolPlaceCue { x, y } => {
                self.handle_pool_place_cue(x, y);
                Ok(())
            }
            ClientMessage::PoolSettings { settings } => {
                self.handle_pool_settings(settings);
                Ok(())
            }
            ClientMessage::PoolRerack {} => {
                self.handle_pool_rerack();
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
            ClientMessage::AuthEmail { .. }
            | ClientMessage::AuthMagic { .. }
            | ClientMessage::AuthGoogle { .. }
            | ClientMessage::AuthLogout {}
            | ClientMessage::AccountUpdate { .. }
            | ClientMessage::AccountDelete {}
            | ClientMessage::ProfileGet { .. }
            | ClientMessage::HighscoresGet { .. } => {
                self.push_error(ErrorCode::NotAllowed, "offline");
                Ok(())
            }
        }
    }

    fn poll_messages(&mut self) -> Vec<ServerMessage> {
        self.outbox.drain(..).collect()
    }

    fn tick(&mut self, dt_ms: u32) {
        self.tick_checkers(dt_ms);
        self.tick_pool(dt_ms);
    }
}
