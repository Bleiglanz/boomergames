//! Standalone wire protocol DTOs for Chess.

use serde::{Deserialize, Serialize};

use crate::checkers::CheckersClockModeDto;
use crate::envelope::SeatDto;

/// Configurable Chess match clock settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChessSettingsDto {
    /// Active clock mode (`move`, `game`, or `unlimited`).
    pub clock_mode: CheckersClockModeDto,
    /// Seconds per move when `clock_mode == Move`.
    pub move_timer_secs: u32,
    /// Minutes per player when `clock_mode == Game`.
    pub game_minutes: u32,
}

impl Default for ChessSettingsDto {
    fn default() -> Self {
        Self {
            clock_mode: CheckersClockModeDto::Game,
            move_timer_secs: 30,
            game_minutes: 10,
        }
    }
}

/// Wire representation of Chess piece color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChessColorDto {
    /// White moves first.
    White,
    /// Black moves second.
    Black,
}

/// Wire representation of a legal Chess move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChessMoveDto {
    /// Origin square index (`0 = a1` .. `63 = h8`).
    pub from: u8,
    /// Destination square index (`0 = a1` .. `63 = h8`).
    pub to: u8,
    /// UCI string (`e2e4`, `e7e8q`).
    pub uci: String,
    /// Optional promotion letter (`"q"`, `"r"`, `"b"`, `"n"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion: Option<String>,
    /// `true` if this move captures an opponent piece.
    pub is_capture: bool,
}

/// Terminal win reason in Chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChessWinReasonDto {
    /// Opponent king checkmated.
    Checkmate,
    /// Opponent resigned.
    Resign,
    /// Opponent ran out of time.
    Timeout,
}

/// Terminal draw reason in Chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChessDrawReasonDto {
    /// Stalemate.
    Stalemate,
    /// Mutual draw agreement.
    Agreement,
    /// Threefold repetition.
    ThreefoldRepetition,
    /// 50-move rule.
    FiftyMoveRule,
    /// Insufficient mating material.
    InsufficientMaterial,
}

/// Terminal Chess match outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChessOutcomeDto {
    /// Decisive victory.
    Win {
        /// Winning color.
        winner: ChessColorDto,
        /// Winning seat (`1` or `2`).
        winner_seat: SeatDto,
        /// Win reason.
        reason: ChessWinReasonDto,
    },
    /// Drawn match.
    Draw {
        /// Draw reason.
        reason: ChessDrawReasonDto,
    },
}

/// Complete Chess game state snapshot synchronized from transport to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChessStateDto {
    /// 64-character board string ordered `a8..h8`, ..., `a1..h1` (`PNBRQKpnbrqk.`).
    pub board: String,
    /// Full FEN string (fed directly to Stockfish in single-player mode).
    pub fen: String,
    /// Color whose turn it currently is.
    pub turn: ChessColorDto,
    /// Seat whose turn it currently is.
    pub active_seat: SeatDto,
    /// Color assigned to `[Seat 1, Seat 2]` (alternates on rematch).
    pub seat_colors: [ChessColorDto; 2],
    /// Monotonic turn counter (starts at 1).
    pub turn_number: u32,
    /// SAN move list (`e4`, `e5`, `Nf3`, `O-O`, ...).
    pub moves: Vec<String>,
    /// Last played `[from, to]` square indices (`0..64`), if any.
    pub last_move: Option<[u8; 2]>,
    /// `true` if the active side's king is currently in check.
    pub in_check: bool,
    /// Square index (`0..64`) of the king currently in check, if any.
    pub check_square: Option<u8>,
    /// All legal moves for the active side.
    pub legal_moves: Vec<ChessMoveDto>,
    /// Pieces captured by White (`"p"`, `"n"`, `"b"`, `"r"`, `"q"`).
    pub captured_by_white: Vec<String>,
    /// Pieces captured by Black (`"P"`, `"N"`, `"B"`, `"R"`, `"Q"`).
    pub captured_by_black: Vec<String>,
    /// Active clock settings.
    pub settings: ChessSettingsDto,
    /// Remaining milliseconds per seat (`[Seat 1, Seat 2]`).
    pub clocks_ms: [u32; 2],
    /// Remaining milliseconds for the active move in `Move` clock mode.
    pub turn_ms_left: u32,
    /// Seat that currently has an unanswered draw offer pending.
    pub draw_offered_by: Option<SeatDto>,
    /// `true` while Stockfish / House Bot is thinking.
    pub bot_thinking: bool,
    /// Match outcome, or `None` while in progress.
    pub outcome: Option<ChessOutcomeDto>,
}
