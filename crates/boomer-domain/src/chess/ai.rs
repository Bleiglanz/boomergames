//! Lightweight deterministic Rust Chess AI (alpha-beta + piece-square tables).
//!
//! Used for server-side online House Bot tables and deterministic Rust tests.
//! Browser single-player Chess uses `nmrugg/stockfish.js` (Stockfish 19 WASM Web Worker).

use crate::BotLevel;
use crate::chess::board::{ChessColor, ChessPieceKind, ChessPosition, ChessSquare};
use crate::chess::rules::{ChessMove, apply_move_unchecked, generate_legal_moves};
use crate::rng::SplitMix64;

const INF: i32 = 1_000_000;

/// Deterministic alpha-beta Chess search for server House Bots and test fallback.
#[derive(Debug, Clone)]
pub struct ChessSearch {
    root: ChessPosition,
    depth: u8,
    rng: SplitMix64,
}

impl ChessSearch {
    /// Creates a new Chess search at `level`.
    #[must_use]
    pub fn new(root: &ChessPosition, level: BotLevel, seed: u64) -> Self {
        let depth = match level {
            BotLevel::Easy => 1,
            BotLevel::Medium => 2,
            BotLevel::Hard => 3,
        };
        Self {
            root: root.clone(),
            depth,
            rng: SplitMix64::new(seed),
        }
    }

    /// Runs the search to completion and returns the selected legal move, if any.
    #[must_use]
    pub fn run_to_completion(&mut self) -> Option<ChessMove> {
        let mut moves = generate_legal_moves(&self.root);
        if moves.is_empty() {
            return None;
        }
        if moves.len() == 1 {
            return moves.first().copied();
        }
        order_moves(&self.root, &mut moves);

        let mut best_move = *moves.first()?;
        let mut best_score = -INF - 1;
        let mut alpha = -INF;
        let beta = INF;

        for mv in moves {
            let next = apply_move_unchecked(&self.root, mv);
            let mut score = -negamax(&next, self.depth.saturating_sub(1), -beta, -alpha, 1);
            // Tiny deterministic jitter on Easy so House Bot (Easy) plays varied openings.
            if self.depth == 1 {
                let jitter = i32::try_from(self.rng.next_u64() % 51).unwrap_or(25) - 25;
                score = score.saturating_add(jitter);
            }
            if score > best_score {
                best_score = score;
                best_move = mv;
            }
            if score > alpha {
                alpha = score;
            }
        }
        Some(best_move)
    }
}

fn negamax(pos: &ChessPosition, depth: u8, mut alpha: i32, beta: i32, ply: i32) -> i32 {
    if pos.is_insufficient_material() || pos.halfmove_clock >= 100 {
        return 0;
    }
    let mut moves = generate_legal_moves(pos);
    if moves.is_empty() {
        return if pos.is_in_check(pos.turn) {
            -(900_000 - ply * 100)
        } else {
            0
        };
    }
    if depth == 0 {
        return evaluate(pos);
    }
    order_moves(pos, &mut moves);

    let mut best = -INF;
    for mv in moves {
        let next = apply_move_unchecked(pos, mv);
        let score = -negamax(&next, depth - 1, -beta, -alpha, ply + 1);
        if score > best {
            best = score;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}

fn order_moves(pos: &ChessPosition, moves: &mut [ChessMove]) {
    moves.sort_by_key(|mv| {
        let mut prio = 0_i32;
        if let Some(promo) = mv.promotion {
            prio += promo.material_value();
        }
        if mv.is_capture {
            let victim = pos.piece_at(mv.to).map_or(100, |p| p.kind.material_value());
            let attacker = pos
                .piece_at(mv.from)
                .map_or(100, |p| p.kind.material_value());
            prio += victim * 10 - attacker;
        }
        if mv.is_castle {
            prio += 60;
        }
        -prio
    });
}

fn evaluate(pos: &ChessPosition) -> i32 {
    let mut score = 0_i32;
    for idx in 0..64_u8 {
        let Some(sq) = ChessSquare::from_index(idx) else {
            continue;
        };
        let Some(piece) = pos.piece_at(sq) else {
            continue;
        };
        let rank_from_own = match piece.color {
            ChessColor::White => sq.rank(),
            ChessColor::Black => 7 - sq.rank(),
        };
        let file_center = 3_i32 - i32::from(sq.file()).abs_diff(3).cast_signed();
        let rank_center = 3_i32 - i32::from(rank_from_own).abs_diff(3).cast_signed();
        let pst = match piece.kind {
            ChessPieceKind::Pawn => i32::from(rank_from_own) * 8 + file_center * 4,
            ChessPieceKind::Knight => (file_center + rank_center) * 9,
            ChessPieceKind::Bishop => (file_center + rank_center) * 6,
            ChessPieceKind::Rook => i32::from(rank_from_own) * 4 + file_center * 3,
            ChessPieceKind::Queen => (file_center + rank_center) * 3,
            ChessPieceKind::King => {
                // Prefer castled king safety on back rank early.
                if rank_from_own == 0 && (sq.file() <= 2 || sq.file() >= 6) {
                    24
                } else {
                    -i32::from(rank_from_own) * 6
                }
            }
        };
        let val = piece.kind.material_value() + pst;
        if piece.color == pos.turn {
            score += val;
        } else {
            score -= val;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_returns_legal_move_from_start() {
        let pos = ChessPosition::starting();
        let mut search = ChessSearch::new(&pos, BotLevel::Medium, 42);
        let mv = search.run_to_completion().unwrap();
        assert!(generate_legal_moves(&pos).contains(&mv));
    }
}
