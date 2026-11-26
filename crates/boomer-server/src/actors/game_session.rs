//! Authoritative server-side game session wrappers (`Checkers` & `8-Ball Pool`)
//! plus bounded `spawn_blocking` bot AI execution.

use std::sync::Arc;

use boomer_domain::checkers::{
    CheckersColor, CheckersGame, CheckersOutcome, CheckersSearch, DrawReason, WinReason,
};
use boomer_domain::chess::{
    ChessColor, ChessDrawReason, ChessGame, ChessOutcome, ChessPosition, ChessSearch,
    ChessWinReason,
};
use boomer_domain::math::Vec2;
use boomer_domain::pool::{PoolGame, PoolSearch, PoolWinReason, ShotInput};
use boomer_domain::{BotLevel, Seat};
use boomer_protocol::{
    BotLevelDto, CheckersClockModeDto, CheckersSettingsDto, CheckersStateDto, ChessSettingsDto,
    ChessStateDto, ErrorCode, PoolBallDto, PoolShotDto, PoolStateDto, SeatDto, ServerMessage,
    TableSettingsDto,
};
use tokio::sync::{Semaphore, mpsc};

use crate::actors::table_actor::TableCmd;
use crate::mapping::{
    ball_group_to_dto, bot_level_from_dto, checkers_color_to_dto, checkers_move_to_dto,
    checkers_outcome_to_dto, checkers_settings_from_table, chess_captured_to_strings,
    chess_color_to_dto, chess_move_to_dto, chess_outcome_to_dto, chess_settings_from_table,
    notation_path_to_squares, pool_balls_to_dto, pool_foul_to_dto, pool_outcome_to_dto,
    pool_settings_from_table, pool_settings_to_dto, pool_shot_from_dto, pool_shot_to_dto,
    seat_to_dto,
};

/// Terminal outcome summary passed to `MatchRepo` for atomic Elo + High Score persistence.
#[derive(Debug, Clone)]
pub struct MatchFinishInfo {
    /// Winning seat (`Some(1)`, `Some(2)`, or `None` for draw).
    pub winner: Option<i16>,
    /// Wire winning seat (`None` for draw).
    pub winner_seat: Option<SeatDto>,
    /// Short finish reason string for DB & `game.over`.
    pub reason: String,
    /// Move/shot count for High Score minimum-activity gate.
    pub activity_count: u32,
}

/// Bot computation payload sent back asynchronously from the bounded `spawn_blocking` pool.
#[derive(Debug, Clone)]
pub enum BotDecision {
    /// Selected Checkers move path (`1..=32` notation).
    CheckersMove(Vec<u8>),
    /// Selected Pool shot.
    PoolShot(PoolShotDto),
    /// Selected Chess UCI move (`e2e4`, `e7e8q`).
    ChessMove(String),
}

/// Active authoritative game engine inside a [`super::table_actor::TableActor`].
pub enum ActiveGame {
    /// Live authoritative English Draughts match.
    Checkers(ServerCheckersMatch),
    /// Live authoritative WPA/BCA 8-ball pool match.
    Pool(Box<ServerPoolMatch>),
    /// Live authoritative FIDE Chess match.
    Chess(Box<ServerChessMatch>),
}

impl ActiveGame {
    /// Returns the currently active turn counter.
    #[must_use]
    pub const fn turn_number(&self) -> u32 {
        match self {
            Self::Checkers(m) => m.turn_number,
            Self::Pool(m) => m.game.turn_number(),
            Self::Chess(m) => m.turn_number,
        }
    }

    /// Returns the active seat index (`0` for Seat 1, `1` for Seat 2).
    #[must_use]
    pub const fn active_seat_index(&self) -> usize {
        match self {
            Self::Checkers(m) => match m.active_seat() {
                Seat::One => 0,
                Seat::Two => 1,
            },
            Self::Pool(m) => match m.game.active_seat() {
                Seat::One => 0,
                Seat::Two => 1,
            },
            Self::Chess(m) => match m.active_seat() {
                Seat::One => 0,
                Seat::Two => 1,
            },
        }
    }

    /// Returns remaining milliseconds on the active turn clock.
    #[must_use]
    pub const fn ms_left(&self) -> u64 {
        match self {
            Self::Checkers(m) => m.ms_left(),
            Self::Pool(m) => m.turn_ms_left as u64,
            Self::Chess(m) => m.ms_left(),
        }
    }

    /// Returns a full game snapshot message for late-joining spectators or reconnecting players.
    #[must_use]
    pub fn snapshot_message(&self, bot_thinking: bool) -> ServerMessage {
        match self {
            Self::Checkers(m) => ServerMessage::CheckersSnapshot {
                state: m.to_dto(bot_thinking),
            },
            Self::Pool(m) => ServerMessage::PoolSnapshot {
                state: m.to_dto(bot_thinking),
            },
            Self::Chess(m) => ServerMessage::ChessSnapshot {
                state: m.to_dto(bot_thinking),
            },
        }
    }

    /// Returns terminal finish metadata if the match has concluded.
    #[must_use]
    pub fn finish_info(&self) -> Option<MatchFinishInfo> {
        match self {
            Self::Checkers(m) => m.finish_info(),
            Self::Pool(m) => m.finish_info(),
            Self::Chess(m) => m.finish_info(),
        }
    }

    /// Advances active clocks by 1 second; returns `true` if a timeout state change occurred.
    pub fn tick_one_sec(&mut self) -> bool {
        match self {
            Self::Checkers(m) => m.tick_one_sec(),
            Self::Pool(m) => m.tick_one_sec(),
            Self::Chess(m) => m.tick_one_sec(),
        }
    }

    /// Forces resignation by `loser_seat_idx` (`0` = Seat 1, `1` = Seat 2).
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if the match has already finished.
    pub fn resign(&mut self, loser_seat_idx: usize) -> Result<(), ErrorCode> {
        let seat = if loser_seat_idx == 0 {
            Seat::One
        } else {
            Seat::Two
        };
        match self {
            Self::Checkers(m) => m.resign(seat),
            Self::Pool(m) => m.resign(seat),
            Self::Chess(m) => m.resign(seat),
        }
    }
}

/// Authoritative Checkers match state.
pub struct ServerCheckersMatch {
    /// Core domain engine.
    pub game: CheckersGame,
    /// Colors assigned to `[Seat 1, Seat 2]` (swapped on each rematch).
    pub seat_colors: [CheckersColor; 2],
    /// Clock settings.
    pub settings: CheckersSettingsDto,
    /// Monotonic turn counter starting at `1`.
    pub turn_number: u32,
    /// Per-seat remaining game clock milliseconds.
    pub clocks_ms: [u32; 2],
    /// Remaining per-move clock milliseconds.
    pub turn_ms_left: u32,
    /// Pending draw offer seat.
    pub draw_offered_by: Option<Seat>,
}

impl ServerCheckersMatch {
    /// Creates a new authoritative Checkers match.
    #[must_use]
    pub fn new(seat_colors: [CheckersColor; 2], table_settings: &TableSettingsDto) -> Self {
        let settings = checkers_settings_from_table(table_settings);
        let game_ms = settings.game_minutes.saturating_mul(60_000);
        let move_ms = settings.move_timer_secs.saturating_mul(1_000);
        Self {
            game: CheckersGame::new(),
            seat_colors,
            settings,
            turn_number: 1,
            clocks_ms: [game_ms, game_ms],
            turn_ms_left: move_ms,
            draw_offered_by: None,
        }
    }

    /// Returns the seat whose color is currently to move.
    #[must_use]
    pub const fn active_seat(&self) -> Seat {
        let [seat_one_color, _] = self.seat_colors;
        if seat_one_color as u8 == self.game.board().turn as u8 {
            Seat::One
        } else {
            Seat::Two
        }
    }

    /// Returns remaining milliseconds on the active clock.
    #[must_use]
    pub const fn ms_left(&self) -> u64 {
        match self.settings.clock_mode {
            CheckersClockModeDto::Unlimited => 0,
            CheckersClockModeDto::Move => self.turn_ms_left as u64,
            CheckersClockModeDto::Game => match self.active_seat() {
                Seat::One => self.clocks_ms[0] as u64,
                Seat::Two => self.clocks_ms[1] as u64,
            },
        }
    }

    /// Validates turn & seat and applies a Checkers move path.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotYourTurn`], [`ErrorCode::StaleTurn`], or [`ErrorCode::IllegalAction`].
    pub fn apply_move(&mut self, seat_idx: usize, turn: u32, path: &[u8]) -> Result<(), ErrorCode> {
        let expected_idx = usize::from(matches!(self.active_seat(), Seat::Two));
        if seat_idx != expected_idx {
            return Err(ErrorCode::NotYourTurn);
        }
        if turn != self.turn_number {
            return Err(ErrorCode::StaleTurn);
        }
        let Some(squares) = notation_path_to_squares(path) else {
            return Err(ErrorCode::IllegalAction);
        };
        if self.game.apply_path(&squares).is_err() {
            return Err(ErrorCode::IllegalAction);
        }
        self.turn_number = self.turn_number.saturating_add(1);
        self.turn_ms_left = self.settings.move_timer_secs.saturating_mul(1_000);
        self.draw_offered_by = None;
        Ok(())
    }

    /// Offers a draw on behalf of `seat`.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if the game ended or a draw is already pending.
    pub fn offer_draw(&mut self, seat: Seat) -> Result<(), ErrorCode> {
        if self.game.outcome().is_some() || self.draw_offered_by.is_some() {
            return Err(ErrorCode::NotAllowed);
        }
        self.draw_offered_by = Some(seat);
        Ok(())
    }

    /// Answers a pending draw offer from the opposing seat.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if no opponent draw offer is pending.
    pub fn answer_draw(&mut self, responder: Seat, accept: bool) -> Result<(), ErrorCode> {
        let Some(offerer) = self.draw_offered_by else {
            return Err(ErrorCode::NotAllowed);
        };
        if offerer == responder {
            return Err(ErrorCode::NotAllowed);
        }
        self.draw_offered_by = None;
        if accept {
            self.game.agree_draw().map_err(|_| ErrorCode::NotAllowed)?;
        }
        Ok(())
    }

    /// Resigns on behalf of `loser`.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if the match is already over.
    pub fn resign(&mut self, loser: Seat) -> Result<(), ErrorCode> {
        let color = match loser {
            Seat::One => self.seat_colors[0],
            Seat::Two => self.seat_colors[1],
        };
        self.game.resign(color).map_err(|_| ErrorCode::NotAllowed)
    }

    /// Advances the active clock by 1 second; triggers timeout loss when it hits zero.
    pub fn tick_one_sec(&mut self) -> bool {
        if self.game.outcome().is_some() {
            return false;
        }
        match self.settings.clock_mode {
            CheckersClockModeDto::Unlimited => false,
            CheckersClockModeDto::Move => {
                self.turn_ms_left = self.turn_ms_left.saturating_sub(1_000);
                if self.turn_ms_left == 0 {
                    let _ = self.game.timeout(self.game.board().turn);
                    return true;
                }
                false
            }
            CheckersClockModeDto::Game => {
                let idx = usize::from(matches!(self.active_seat(), Seat::Two));
                if let Some(clock) = self.clocks_ms.get_mut(idx) {
                    *clock = clock.saturating_sub(1_000);
                    if *clock == 0 {
                        let _ = self.game.timeout(self.game.board().turn);
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Builds a [`CheckersStateDto`] for broadcast.
    #[must_use]
    pub fn to_dto(&self, bot_thinking: bool) -> CheckersStateDto {
        let (cap_black, cap_white) = self.game.captured_counts();
        CheckersStateDto {
            board: self.game.board().to_board_string(),
            turn: checkers_color_to_dto(self.game.board().turn),
            active_seat: seat_to_dto(self.active_seat()),
            seat_colors: [
                checkers_color_to_dto(self.seat_colors[0]),
                checkers_color_to_dto(self.seat_colors[1]),
            ],
            turn_number: self.turn_number,
            moves: self.game.move_history().to_vec(),
            legal_moves: self
                .game
                .legal_moves()
                .iter()
                .map(checkers_move_to_dto)
                .collect(),
            mandatory_capture: self.game.has_mandatory_capture(),
            captured_by_black: cap_black,
            captured_by_white: cap_white,
            last_move: self
                .game
                .last_move()
                .map(|mv| mv.path.iter().map(|sq| sq.notation()).collect()),
            draw_offered_by: self.draw_offered_by.map(seat_to_dto),
            outcome: self
                .game
                .outcome()
                .map(|o| checkers_outcome_to_dto(o, self.seat_colors)),
            settings: self.settings,
            clocks_ms: self.clocks_ms,
            turn_ms_left: self.turn_ms_left,
            bot_thinking,
        }
    }

    /// Extracts terminal persistence summary if the match ended.
    #[must_use]
    pub fn finish_info(&self) -> Option<MatchFinishInfo> {
        let outcome = self.game.outcome()?;
        let activity_count = u32::try_from(self.game.move_history().len()).unwrap_or(u32::MAX);
        match outcome {
            CheckersOutcome::Win { winner, reason } => {
                let [seat_one_color, _] = self.seat_colors;
                let (winner_i16, winner_seat) = if seat_one_color as u8 == winner as u8 {
                    (1, SeatDto::One)
                } else {
                    (2, SeatDto::Two)
                };
                let reason_str = match reason {
                    WinReason::NoMoves => "no_moves",
                    WinReason::Resign => "resign",
                    WinReason::Timeout => "timeout",
                };
                Some(MatchFinishInfo {
                    winner: Some(winner_i16),
                    winner_seat: Some(winner_seat),
                    reason: reason_str.to_owned(),
                    activity_count,
                })
            }
            CheckersOutcome::Draw { reason } => {
                let reason_str = match reason {
                    DrawReason::Agreement => "agreement",
                    DrawReason::ThreefoldRepetition => "threefold_repetition",
                    DrawReason::FortyMoveRule => "forty_move_rule",
                };
                Some(MatchFinishInfo {
                    winner: None,
                    winner_seat: None,
                    reason: reason_str.to_owned(),
                    activity_count,
                })
            }
        }
    }
}

/// Authoritative 8-Ball Pool match state.
pub struct ServerPoolMatch {
    /// Core domain engine.
    pub game: PoolGame,
    /// Remaining shot clock milliseconds (`0` when unlimited).
    pub turn_ms_left: u32,
    /// Most recent shot fouls for HUD feedback.
    pub last_fouls: Vec<boomer_domain::pool::PoolFoul>,
    /// Object balls pocketed on the most recent shot.
    pub last_potted: Vec<u8>,
    /// Whether the most recent shot triggered an 8-on-break re-rack.
    pub reracked: bool,
    /// Total shots executed in this rack.
    pub shots_taken: u32,
}

impl ServerPoolMatch {
    /// Starts a new deterministic 8-ball rack.
    #[must_use]
    pub fn new(breaker: Seat, table_settings: &TableSettingsDto, seed: u64) -> Self {
        let settings = pool_settings_from_table(table_settings);
        let ms = settings.shot_timer_secs.saturating_mul(1_000);
        Self {
            game: PoolGame::new(breaker, settings, false, seed),
            turn_ms_left: ms,
            last_fouls: Vec::new(),
            last_potted: Vec::new(),
            reracked: false,
            shots_taken: 0,
        }
    }

    /// Places the cue ball during ball-in-hand.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotYourTurn`] or [`ErrorCode::IllegalAction`].
    pub fn place_cue(&mut self, seat_idx: usize, x: f64, y: f64) -> Result<(), ErrorCode> {
        let expected_idx = usize::from(matches!(self.game.active_seat(), Seat::Two));
        if seat_idx != expected_idx {
            return Err(ErrorCode::NotYourTurn);
        }
        self.game
            .place_cue_ball(Vec2::new(x, y))
            .map_err(|_| ErrorCode::IllegalAction)
    }

    /// Validates turn & seat, executes authoritative deterministic Swept-CCD simulation,
    /// and returns `(pre_balls, applied_shot, post_state)` for client replay + snap.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotYourTurn`], [`ErrorCode::StaleTurn`], or [`ErrorCode::IllegalAction`].
    pub fn shoot(
        &mut self,
        seat_idx: usize,
        shot: PoolShotDto,
    ) -> Result<(Vec<PoolBallDto>, PoolShotDto, PoolStateDto), ErrorCode> {
        let expected_idx = usize::from(matches!(self.game.active_seat(), Seat::Two));
        if seat_idx != expected_idx {
            return Err(ErrorCode::NotYourTurn);
        }
        if shot.turn != self.game.turn_number() {
            return Err(ErrorCode::StaleTurn);
        }
        let pre_balls = pool_balls_to_dto(self.game.balls());
        let input: ShotInput = pool_shot_from_dto(shot);
        let outcome = self
            .game
            .shoot(input, false)
            .map_err(|_| ErrorCode::IllegalAction)?;

        self.last_fouls = outcome.fouls;
        self.last_potted = outcome.potted.iter().map(|&(ball_id, _)| ball_id).collect();
        self.reracked = outcome.reracked;
        self.shots_taken = self.shots_taken.saturating_add(1);
        self.turn_ms_left = self.game.settings().shot_timer_secs.saturating_mul(1_000);
        let post_state = self.to_dto(false);
        Ok((pre_balls, shot, post_state))
    }

    /// Resigns on behalf of `loser`.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if already finished.
    pub fn resign(&mut self, loser: Seat) -> Result<(), ErrorCode> {
        self.game.resign(loser).map_err(|_| ErrorCode::NotAllowed)
    }

    /// Advances shot clock by 1 second; expiry grants opponent Ball-in-Hand (per D-9).
    pub fn tick_one_sec(&mut self) -> bool {
        if self.game.outcome().is_some() || self.game.settings().shot_timer_secs == 0 {
            return false;
        }
        self.turn_ms_left = self.turn_ms_left.saturating_sub(1_000);
        if self.turn_ms_left == 0 {
            let _ = self.game.timeout_turn();
            self.last_fouls.clear();
            self.last_potted.clear();
            self.reracked = false;
            self.turn_ms_left = self.game.settings().shot_timer_secs.saturating_mul(1_000);
            return true;
        }
        false
    }

    /// Builds a [`PoolStateDto`] for broadcast.
    #[must_use]
    pub fn to_dto(&self, bot_thinking: bool) -> PoolStateDto {
        let [g1, g2] = self.game.groups();
        let [p1, p2] = self.game.pocketed_by_seat();
        PoolStateDto {
            balls: pool_balls_to_dto(self.game.balls()),
            active_seat: seat_to_dto(self.game.active_seat()),
            groups: [g1.map(ball_group_to_dto), g2.map(ball_group_to_dto)],
            pocketed_by_seat: [p1.clone(), p2.clone()],
            is_break_shot: self.game.is_break_shot(),
            ball_in_hand: self.game.ball_in_hand(),
            ball_in_hand_behind_line: self.game.ball_in_hand_behind_line(),
            is_on_eight_ball: self.game.is_on_eight_ball(self.game.active_seat()),
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
            outcome: self.game.outcome().map(pool_outcome_to_dto),
            free_practice: false,
            bot_thinking,
        }
    }

    /// Extracts terminal persistence summary if the rack finished.
    #[must_use]
    pub fn finish_info(&self) -> Option<MatchFinishInfo> {
        let outcome = self.game.outcome()?;
        let (winner_i16, winner_seat) = match outcome.winner {
            Seat::One => (1, SeatDto::One),
            Seat::Two => (2, SeatDto::Two),
        };
        let reason_str = match outcome.reason {
            PoolWinReason::EightBallPocketed => "eight_ball_pocketed",
            PoolWinReason::OpponentEarlyEight => "opponent_early_eight",
            PoolWinReason::OpponentFoulOnEight => "opponent_foul_on_eight",
            PoolWinReason::Resign => "resign",
        };
        Some(MatchFinishInfo {
            winner: Some(winner_i16),
            winner_seat: Some(winner_seat),
            reason: reason_str.to_owned(),
            activity_count: self.shots_taken,
        })
    }
}

/// Authoritative FIDE Chess match state.
pub struct ServerChessMatch {
    /// Core domain engine.
    pub game: ChessGame,
    /// Colors assigned to `[Seat 1, Seat 2]` (swapped on each rematch).
    pub seat_colors: [ChessColor; 2],
    /// Clock settings.
    pub settings: ChessSettingsDto,
    /// Monotonic turn counter starting at `1`.
    pub turn_number: u32,
    /// Per-seat remaining game clock milliseconds.
    pub clocks_ms: [u32; 2],
    /// Remaining per-move clock milliseconds.
    pub turn_ms_left: u32,
    /// Pending draw offer seat.
    pub draw_offered_by: Option<Seat>,
}

impl ServerChessMatch {
    /// Creates a new authoritative Chess match.
    #[must_use]
    pub fn new(seat_colors: [ChessColor; 2], table_settings: &TableSettingsDto) -> Self {
        let settings = chess_settings_from_table(table_settings);
        let game_ms = settings.game_minutes.saturating_mul(60_000);
        let move_ms = settings.move_timer_secs.saturating_mul(1_000);
        Self {
            game: ChessGame::new(),
            seat_colors,
            settings,
            turn_number: 1,
            clocks_ms: [game_ms, game_ms],
            turn_ms_left: move_ms,
            draw_offered_by: None,
        }
    }

    /// Returns the seat whose color is currently to move.
    #[must_use]
    pub const fn active_seat(&self) -> Seat {
        let [seat_one_color, _] = self.seat_colors;
        if seat_one_color as u8 == self.game.position().turn as u8 {
            Seat::One
        } else {
            Seat::Two
        }
    }

    /// Returns remaining milliseconds on the active clock.
    #[must_use]
    pub const fn ms_left(&self) -> u64 {
        match self.settings.clock_mode {
            CheckersClockModeDto::Unlimited => 0,
            CheckersClockModeDto::Move => self.turn_ms_left as u64,
            CheckersClockModeDto::Game => match self.active_seat() {
                Seat::One => self.clocks_ms[0] as u64,
                Seat::Two => self.clocks_ms[1] as u64,
            },
        }
    }

    /// Validates turn & seat and applies a UCI move string.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotYourTurn`], [`ErrorCode::StaleTurn`], or [`ErrorCode::IllegalAction`].
    pub fn apply_move(&mut self, seat_idx: usize, turn: u32, uci: &str) -> Result<(), ErrorCode> {
        let expected_idx = usize::from(matches!(self.active_seat(), Seat::Two));
        if seat_idx != expected_idx {
            return Err(ErrorCode::NotYourTurn);
        }
        if turn != self.turn_number {
            return Err(ErrorCode::StaleTurn);
        }
        if self.game.apply_uci(uci).is_err() {
            return Err(ErrorCode::IllegalAction);
        }
        self.turn_number = self.turn_number.saturating_add(1);
        self.turn_ms_left = self.settings.move_timer_secs.saturating_mul(1_000);
        self.draw_offered_by = None;
        Ok(())
    }

    /// Offers a draw on behalf of `seat`.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if already ended or draw already offered.
    pub fn offer_draw(&mut self, seat: Seat) -> Result<(), ErrorCode> {
        if self.game.outcome().is_some() || self.draw_offered_by.is_some() {
            return Err(ErrorCode::NotAllowed);
        }
        self.draw_offered_by = Some(seat);
        Ok(())
    }

    /// Answers a pending draw offer from the opposing seat.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if no opponent draw offer is pending.
    pub fn answer_draw(&mut self, responder: Seat, accept: bool) -> Result<(), ErrorCode> {
        let Some(offerer) = self.draw_offered_by else {
            return Err(ErrorCode::NotAllowed);
        };
        if offerer == responder {
            return Err(ErrorCode::NotAllowed);
        }
        self.draw_offered_by = None;
        if accept {
            self.game.agree_draw().map_err(|_| ErrorCode::NotAllowed)?;
        }
        Ok(())
    }

    /// Resigns on behalf of `loser`.
    ///
    /// # Errors
    /// Returns [`ErrorCode::NotAllowed`] if the match already ended.
    pub fn resign(&mut self, loser: Seat) -> Result<(), ErrorCode> {
        let color = match loser {
            Seat::One => self.seat_colors[0],
            Seat::Two => self.seat_colors[1],
        };
        self.game.resign(color).map_err(|_| ErrorCode::NotAllowed)
    }

    /// Advances the active clock by 1 second; triggers timeout loss when it hits zero.
    pub fn tick_one_sec(&mut self) -> bool {
        if self.game.outcome().is_some() {
            return false;
        }
        match self.settings.clock_mode {
            CheckersClockModeDto::Unlimited => false,
            CheckersClockModeDto::Move => {
                self.turn_ms_left = self.turn_ms_left.saturating_sub(1_000);
                if self.turn_ms_left == 0 {
                    let _ = self.game.timeout(self.game.position().turn);
                    return true;
                }
                false
            }
            CheckersClockModeDto::Game => {
                let idx = usize::from(matches!(self.active_seat(), Seat::Two));
                if let Some(clock) = self.clocks_ms.get_mut(idx) {
                    *clock = clock.saturating_sub(1_000);
                    if *clock == 0 {
                        let _ = self.game.timeout(self.game.position().turn);
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Builds a [`ChessStateDto`] for broadcast.
    #[must_use]
    pub fn to_dto(&self, bot_thinking: bool) -> ChessStateDto {
        let pos = self.game.position();
        let in_check = self.game.in_check();
        let check_square = if in_check {
            pos.king_square(pos.turn)
                .map(boomer_domain::chess::ChessSquare::raw)
        } else {
            None
        };
        ChessStateDto {
            board: pos.to_board_string(),
            fen: pos.to_fen(),
            turn: chess_color_to_dto(pos.turn),
            active_seat: seat_to_dto(self.active_seat()),
            seat_colors: [
                chess_color_to_dto(self.seat_colors[0]),
                chess_color_to_dto(self.seat_colors[1]),
            ],
            turn_number: self.turn_number,
            moves: self.game.move_history().to_vec(),
            last_move: self.game.last_move().map(|m| [m.from.raw(), m.to.raw()]),
            in_check,
            check_square,
            legal_moves: self
                .game
                .legal_moves()
                .iter()
                .copied()
                .map(chess_move_to_dto)
                .collect(),
            captured_by_white: chess_captured_to_strings(
                self.game.captured_by_white(),
                ChessColor::Black,
            ),
            captured_by_black: chess_captured_to_strings(
                self.game.captured_by_black(),
                ChessColor::White,
            ),
            settings: self.settings,
            clocks_ms: self.clocks_ms,
            turn_ms_left: self.turn_ms_left,
            draw_offered_by: self.draw_offered_by.map(seat_to_dto),
            bot_thinking,
            outcome: self
                .game
                .outcome()
                .map(|o| chess_outcome_to_dto(o, self.seat_colors)),
        }
    }

    /// Extracts terminal persistence summary if the match ended.
    #[must_use]
    pub fn finish_info(&self) -> Option<MatchFinishInfo> {
        let outcome = self.game.outcome()?;
        let activity_count = u32::try_from(self.game.move_history().len()).unwrap_or(u32::MAX);
        match outcome {
            ChessOutcome::Win { winner, reason } => {
                let [seat_one_color, _] = self.seat_colors;
                let (winner_i16, winner_seat) = if seat_one_color as u8 == winner as u8 {
                    (1, SeatDto::One)
                } else {
                    (2, SeatDto::Two)
                };
                let reason_str = match reason {
                    ChessWinReason::Checkmate => "checkmate",
                    ChessWinReason::Resign => "resign",
                    ChessWinReason::Timeout => "timeout",
                };
                Some(MatchFinishInfo {
                    winner: Some(winner_i16),
                    winner_seat: Some(winner_seat),
                    reason: reason_str.to_owned(),
                    activity_count,
                })
            }
            ChessOutcome::Draw { reason } => {
                let reason_str = match reason {
                    ChessDrawReason::Stalemate => "stalemate",
                    ChessDrawReason::Agreement => "agreement",
                    ChessDrawReason::ThreefoldRepetition => "threefold_repetition",
                    ChessDrawReason::FiftyMoveRule => "fifty_move_rule",
                    ChessDrawReason::InsufficientMaterial => "insufficient_material",
                };
                Some(MatchFinishInfo {
                    winner: None,
                    winner_seat: None,
                    reason: reason_str.to_owned(),
                    activity_count,
                })
            }
        }
    }
}

/// Request dispatched onto the bounded `spawn_blocking` AI worker pool.
pub enum BotWorkRequest {
    /// Compute the next Checkers move from a snapshot of the board.
    Checkers {
        /// Board snapshot.
        board: boomer_domain::checkers::Board,
        /// Bot difficulty.
        level: BotLevelDto,
        /// Deterministic seed.
        seed: u64,
    },
    /// Compute the next Pool shot from a snapshot of the game.
    Pool {
        /// Game snapshot.
        game: Box<PoolGame>,
        /// Bot difficulty.
        level: BotLevelDto,
        /// Deterministic seed.
        seed: u64,
    },
    /// Compute the next Chess move from a snapshot of the position.
    Chess {
        /// Position snapshot.
        position: Box<ChessPosition>,
        /// Bot difficulty.
        level: BotLevelDto,
        /// Deterministic seed.
        seed: u64,
    },
}

/// Spawns a bounded blocking AI calculation and delivers [`TableCmd::BotDecision`] back to the table.
pub fn spawn_bot_turn(
    semaphore: Arc<Semaphore>,
    table_tx: mpsc::Sender<TableCmd>,
    turn: u32,
    request: BotWorkRequest,
) {
    tokio::spawn(async move {
        let Ok(_permit) = semaphore.acquire_owned().await else {
            return;
        };
        let decision = tokio::task::spawn_blocking(move || match request {
            BotWorkRequest::Checkers { board, level, seed } => {
                let domain_level: BotLevel = bot_level_from_dto(level);
                let mut search = CheckersSearch::new(&board, domain_level, seed);
                search.run_to_completion().map(|mv| {
                    BotDecision::CheckersMove(mv.path.iter().map(|sq| sq.notation()).collect())
                })
            }
            BotWorkRequest::Pool { game, level, seed } => {
                let domain_level: BotLevel = bot_level_from_dto(level);
                let mut search = PoolSearch::new(&game, domain_level, seed);
                let shot = search.run_to_completion();
                Some(BotDecision::PoolShot(pool_shot_to_dto(turn, shot)))
            }
            BotWorkRequest::Chess {
                position,
                level,
                seed,
            } => {
                let domain_level: BotLevel = bot_level_from_dto(level);
                let mut search = ChessSearch::new(&position, domain_level, seed);
                search
                    .run_to_completion()
                    .map(|mv| BotDecision::ChessMove(mv.to_uci()))
            }
        })
        .await
        .ok()
        .flatten();

        if let Some(decision) = decision {
            let _ = table_tx
                .send(TableCmd::BotDecision { turn, decision })
                .await;
        }
    });
}
