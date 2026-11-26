//! Pure FIDE Chess (8x8) rules engine, FEN/SAN serialization, and deterministic fallback AI.

pub mod ai;
pub mod board;
pub mod rules;

pub use ai::ChessSearch;
pub use board::{
    CastlingRights, ChessColor, ChessPiece, ChessPieceKind, ChessPosition, ChessPositionKey,
    ChessSquare,
};
pub use rules::{
    ChessDrawReason, ChessError, ChessGame, ChessMove, ChessOutcome, ChessWinReason,
    apply_move_unchecked, format_san, generate_legal_moves,
};
