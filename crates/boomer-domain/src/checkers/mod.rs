//! Pure English Draughts / American Checkers (8x8) rules engine and time-sliced AI.

pub mod ai;
pub mod board;
pub mod rules;

pub use ai::{CheckersSearch, SearchProgress};
pub use board::{Board, CheckersColor, Piece, PositionKey, Square};
pub use rules::{
    CheckersError, CheckersGame, CheckersMove, CheckersOutcome, DrawReason, WinReason,
    generate_legal_moves,
};
