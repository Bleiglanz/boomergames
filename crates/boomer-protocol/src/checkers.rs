//! Standalone wire protocol DTOs for Checkers (English Draughts).

use serde::{Deserialize, Serialize};

use crate::envelope::SeatDto;

/// Clock mode configured for a Checkers match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckersClockModeDto {
    /// Fixed time limit per individual move (e.g., 20s, 30s, 60s).
    Move,
    /// Shared per-player chess-style game clock (e.g., 5m, 10m, 15m).
    Game,
    /// No clock enforcement.
    Unlimited,
}

/// Configurable Checkers match rules/clock settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckersSettingsDto {
    /// Active clock mode.
    pub clock_mode: CheckersClockModeDto,
    /// Seconds per move when `clock_mode == Move`.
    pub move_timer_secs: u32,
    /// Minutes per player when `clock_mode == Game`.
    pub game_minutes: u32,
}

impl Default for CheckersSettingsDto {
    fn default() -> Self {
        Self {
            clock_mode: CheckersClockModeDto::Game,
            move_timer_secs: 30,
            game_minutes: 10,
        }
    }
}

/// Wire representation of Checkers piece color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckersColorDto {
    /// Dark pieces starting on squares `1..=12`, moving first.
    Black,
    /// Red/Light pieces starting on squares `21..=32`.
    White,
}

/// Wire representation of a legal Checkers move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckersMoveDto {
    /// Ordered `1..=32` dark-square notation path (`[from, to]` or `[from, hop1, hop2, ...]`).
    pub path: Vec<u8>,
    /// Ordered `1..=32` captured squares.
    pub captured: Vec<u8>,
    /// Whether the moving piece was crowned king.
    pub crowned: bool,
    /// Standard algebraic notation (`11-15`, `22x15`, `22x15x8`).
    pub notation: String,
}

/// Terminal win reason in Checkers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckersWinReasonDto {
    /// Opponent had no pieces or legal moves remaining.
    NoMoves,
    /// Opponent resigned.
    Resign,
    /// Opponent ran out of time.
    Timeout,
}

/// Terminal draw reason in Checkers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckersDrawReasonDto {
    /// Mutual draw agreement.
    Agreement,
    /// Threefold position repetition.
    ThreefoldRepetition,
    /// 40 moves per side without capture or man move.
    FortyMoveRule,
}

/// Terminal Checkers match outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckersOutcomeDto {
    /// Decisive victory.
    Win {
        /// Winning color.
        winner: CheckersColorDto,
        /// Winning seat (`1` or `2`).
        winner_seat: SeatDto,
        /// Reason for win.
        reason: CheckersWinReasonDto,
    },
    /// Drawn match.
    Draw {
        /// Draw condition met.
        reason: CheckersDrawReasonDto,
    },
}

/// Complete Checkers game state snapshot synchronized from transport to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckersStateDto {
    /// 32-character board string (`b`, `B`, `w`, `W`, `.`).
    pub board: String,
    /// Color whose turn it currently is.
    pub turn: CheckersColorDto,
    /// Seat whose turn it currently is.
    pub active_seat: SeatDto,
    /// Color assigned to `[Seat 1, Seat 2]` (alternates on rematch).
    pub seat_colors: [CheckersColorDto; 2],
    /// Monotonic turn counter (starts at 1).
    pub turn_number: u32,
    /// Standard notation move list.
    pub moves: Vec<String>,
    /// Last played move path (`1..=32`), if any.
    pub last_move: Option<Vec<u8>>,
    /// `true` if the active side must execute a capture move.
    pub mandatory_capture: bool,
    /// All legal moves for the active side (empty if game over or bot's turn).
    pub legal_moves: Vec<CheckersMoveDto>,
    /// Opponent pieces captured by Black (`0..=12`).
    pub captured_by_black: u8,
    /// Opponent pieces captured by White (`0..=12`).
    pub captured_by_white: u8,
    /// Active clock configuration.
    pub settings: CheckersSettingsDto,
    /// Remaining milliseconds per seat (`[Seat 1, Seat 2]`) in `Game` mode, or current move ms left.
    pub clocks_ms: [u32; 2],
    /// Remaining milliseconds for the active move in `Move` clock mode.
    pub turn_ms_left: u32,
    /// Seat that currently has an unanswered draw offer pending.
    pub draw_offered_by: Option<SeatDto>,
    /// `true` while the computer opponent is thinking.
    pub bot_thinking: bool,
    /// Match outcome, or `None` while in progress.
    pub outcome: Option<CheckersOutcomeDto>,
}
