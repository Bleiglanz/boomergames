//! Time-sliced deterministic alpha-beta minimax AI for English Draughts.
//!
//! Designed to step cleanly in small budgets per animation frame on WASM
//! (`~6-8 ms/frame`) while producing bit-identical results when run to
//! completion in one call on a server worker thread.

use crate::BotLevel;
use crate::checkers::board::{Board, CheckersColor, Square};
use crate::checkers::rules::{CheckersMove, apply_move_to_board, generate_legal_moves};
use crate::rng::SplitMix64;

const INF_SCORE: i32 = 100_000;
const WIN_SCORE: i32 = 50_000;
const MAX_QUIESCENCE_DEPTH: u8 = 6;

/// Progress returned by [`CheckersSearch::step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchProgress {
    /// Search needs more `step` calls to reach its target depth.
    InProgress,
    /// Search finished and selected the move (or `None` if no legal moves exist).
    Done(Option<CheckersMove>),
}

/// Resumable iterative-deepening alpha-beta search for Checkers.
#[derive(Debug, Clone)]
pub struct CheckersSearch {
    board: Board,
    level: BotLevel,
    max_depth: u8,
    root_moves: Vec<CheckersMove>,
    current_depth: u8,
    root_index: usize,
    current_depth_scores: Vec<(usize, i32)>,
    completed_scores: Vec<(usize, i32)>,
    alpha: i32,
    rng: SplitMix64,
    finished: bool,
    chosen: Option<CheckersMove>,
}

impl CheckersSearch {
    /// Creates a new resumable search from `board` at `level` using deterministic `seed`.
    #[must_use]
    pub fn new(board: &Board, level: BotLevel, seed: u64) -> Self {
        let mut root_moves = generate_legal_moves(board);
        sort_moves(&mut root_moves);
        let max_depth = match level {
            BotLevel::Easy => 2,
            BotLevel::Medium => 4,
            BotLevel::Hard => 7,
        };
        let finished = root_moves.len() <= 1;
        let chosen = if root_moves.len() == 1 {
            root_moves.first().cloned()
        } else {
            None
        };
        let move_count = root_moves.len();
        Self {
            board: *board,
            level,
            max_depth,
            root_moves,
            current_depth: 1,
            root_index: 0,
            current_depth_scores: Vec::with_capacity(move_count),
            completed_scores: Vec::with_capacity(move_count),
            alpha: -INF_SCORE,
            rng: SplitMix64::new(seed),
            finished,
            chosen,
        }
    }

    /// Runs the search to completion in one synchronous call (ideal for server worker threads).
    #[must_use]
    pub fn run_to_completion(&mut self) -> Option<CheckersMove> {
        loop {
            match self.step(u32::MAX) {
                SearchProgress::InProgress => {}
                SearchProgress::Done(mv) => return mv,
            }
        }
    }

    /// Advances search by evaluating root moves until `node_budget` nodes are visited
    /// or target depth completes.
    ///
    /// Because chunk boundaries only occur *between* complete root moves, the exact same
    /// search tree is evaluated regardless of `node_budget`, preserving native/WASM parity.
    pub fn step(&mut self, node_budget: u32) -> SearchProgress {
        if self.finished {
            return SearchProgress::Done(self.chosen.clone());
        }

        let mut nodes_used = 0_u32;
        while self.current_depth <= self.max_depth {
            while let Some(mv) = self.root_moves.get(self.root_index).cloned() {
                let next_board = apply_move_to_board(&self.board, &mv);
                let mut nodes = 1_u32;
                let score = -negamax(
                    &next_board,
                    self.current_depth.saturating_sub(1),
                    1,
                    -INF_SCORE,
                    -self.alpha,
                    self.level,
                    &mut nodes,
                );
                nodes_used = nodes_used.saturating_add(nodes);
                self.current_depth_scores.push((self.root_index, score));
                if score > self.alpha {
                    self.alpha = score;
                }
                self.root_index += 1;

                if nodes_used >= node_budget {
                    return SearchProgress::InProgress;
                }
            }

            // Completed an entire depth cleanly: save scores and reorder root moves for next depth.
            self.completed_scores.clone_from(&self.current_depth_scores);
            self.current_depth_scores.clear();
            self.reorder_root_by_scores();
            self.root_index = 0;
            self.alpha = -INF_SCORE;
            self.current_depth += 1;

            if nodes_used >= node_budget && self.current_depth <= self.max_depth {
                return SearchProgress::InProgress;
            }
        }

        self.chosen = self.select_final_move();
        self.finished = true;
        SearchProgress::Done(self.chosen.clone())
    }

    fn reorder_root_by_scores(&mut self) {
        let mut paired: Vec<(CheckersMove, i32)> = self
            .root_moves
            .iter()
            .enumerate()
            .map(|(i, mv)| {
                let score = self
                    .completed_scores
                    .iter()
                    .find_map(|&(idx, sc)| (idx == i).then_some(sc))
                    .unwrap_or(-INF_SCORE);
                (mv.clone(), score)
            })
            .collect();
        paired.sort_by_key(|a| std::cmp::Reverse(a.1));
        self.root_moves = paired.iter().map(|(mv, _)| mv.clone()).collect();
        self.completed_scores = paired
            .iter()
            .enumerate()
            .map(|(i, (_, sc))| (i, *sc))
            .collect();
    }

    fn select_final_move(&mut self) -> Option<CheckersMove> {
        if self.root_moves.is_empty() {
            return None;
        }
        let noise_span: i32 = match self.level {
            BotLevel::Easy => 45,
            BotLevel::Medium => 12,
            BotLevel::Hard => 0,
        };

        let mut ranked: Vec<(usize, i32)> = self
            .completed_scores
            .iter()
            .map(|&(idx, base_score)| {
                let jitter = if noise_span == 0 {
                    0
                } else {
                    #[expect(
                        clippy::cast_sign_loss,
                        reason = "noise_span is a small positive constant (<= 45)"
                    )]
                    let range = (noise_span as u64) * 2 + 1;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "bounded remainder fits easily in i32"
                    )]
                    let offset = self.rng.next_bounded(range) as i32;
                    offset - noise_span
                };
                (idx, base_score.saturating_add(jitter))
            })
            .collect();

        ranked.sort_by_key(|a| std::cmp::Reverse(a.1));

        // On Easy, occasionally pick the second-best move if available.
        let pick_pos = usize::from(
            self.level == BotLevel::Easy && ranked.len() >= 2 && self.rng.next_bounded(4) == 0,
        );

        let best_idx = ranked.get(pick_pos).map_or(0, |&(idx, _)| idx);
        self.root_moves.get(best_idx).cloned()
    }
}

fn sort_moves(moves: &mut [CheckersMove]) {
    moves.sort_by(|a, b| {
        b.captured
            .len()
            .cmp(&a.captured.len())
            .then_with(|| b.crowned.cmp(&a.crowned))
    });
}

fn negamax(
    board: &Board,
    depth: u8,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    level: BotLevel,
    nodes: &mut u32,
) -> i32 {
    *nodes = nodes.saturating_add(1);
    let mut moves = generate_legal_moves(board);
    if moves.is_empty() {
        return -WIN_SCORE + ply;
    }

    if depth == 0 {
        let is_tactical = moves.first().is_some_and(CheckersMove::is_capture);
        if is_tactical {
            return quiescence(board, 0, ply, alpha, beta, level, nodes);
        }
        return evaluate(board, level);
    }

    sort_moves(&mut moves);
    let mut best = -INF_SCORE;
    for mv in &moves {
        let next = apply_move_to_board(board, mv);
        let score = -negamax(&next, depth - 1, ply + 1, -beta, -alpha, level, nodes);
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

fn quiescence(
    board: &Board,
    q_depth: u8,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    level: BotLevel,
    nodes: &mut u32,
) -> i32 {
    *nodes = nodes.saturating_add(1);
    let mut moves = generate_legal_moves(board);
    if moves.is_empty() {
        return -WIN_SCORE + ply;
    }

    let is_capture = moves.first().is_some_and(CheckersMove::is_capture);
    if !is_capture || q_depth >= MAX_QUIESCENCE_DEPTH {
        return evaluate(board, level);
    }

    sort_moves(&mut moves);
    let mut best = -INF_SCORE;
    for mv in &moves {
        let next = apply_move_to_board(board, mv);
        let score = -quiescence(&next, q_depth + 1, ply + 1, -beta, -alpha, level, nodes);
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

/// Evaluates `board` from `board.turn`'s perspective (centipawn-like units).
#[must_use]
fn evaluate(board: &Board, level: BotLevel) -> i32 {
    let black_score = side_score(board, CheckersColor::Black);
    let white_score = side_score(board, CheckersColor::White);
    let mut diff = black_score - white_score;

    // Endgame mop-up heuristic for Medium/Hard: drive lone enemy pieces to edges and close distance.
    if level != BotLevel::Easy {
        let (b_cnt, w_cnt) = board.piece_counts();
        let total = b_cnt + w_cnt;
        if total <= 8 && diff.abs() >= 60 {
            let winner = if diff > 0 {
                CheckersColor::Black
            } else {
                CheckersColor::White
            };
            let mop_up = endgame_mop_up(board, winner);
            if winner == CheckersColor::Black {
                diff += mop_up;
            } else {
                diff -= mop_up;
            }
        }
    }

    match board.turn {
        CheckersColor::Black => diff,
        CheckersColor::White => -diff,
    }
}

fn side_score(board: &Board, color: CheckersColor) -> i32 {
    // Center control mask: notation 10,11,14,15,18,19,22,23 (0-based 9,10,13,14,17,18,21,22)
    const CENTER_MASK: u32 = (1 << 9)
        | (1 << 10)
        | (1 << 13)
        | (1 << 14)
        | (1 << 17)
        | (1 << 18)
        | (1 << 21)
        | (1 << 22);
    const BLACK_BACK_ROW: u32 = 0x0000_000F;
    const WHITE_BACK_ROW: u32 = 0xF000_0000;

    let men = board.men_of(color);
    let kings = board.kings_of(color);
    let mut score = 0_i32;

    // Base material: Man = 100, King = 180
    score += men.count_ones().cast_signed() * 100;
    score += kings.count_ones().cast_signed() * 180;

    // Center control
    score += (men & CENTER_MASK).count_ones().cast_signed() * 10;
    score += (kings & CENTER_MASK).count_ones().cast_signed() * 16;

    // Back-row defense while opponent still has uncrowned men
    if board.men_of(color.opponent()) != 0 {
        let back_mask = match color {
            CheckersColor::Black => BLACK_BACK_ROW,
            CheckersColor::White => WHITE_BACK_ROW,
        };
        score += (men & back_mask).count_ones().cast_signed() * 12;
    }

    // Advancement bonus for men
    let mut rem = men;
    while rem != 0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "trailing_zeros() on non-zero u32 is in 0..32"
        )]
        let idx = rem.trailing_zeros() as u8;
        rem &= rem - 1;
        if let Some(sq) = Square::from_index(idx) {
            let row = sq.row_col().0;
            let progress = match color {
                CheckersColor::Black => i32::from(row),
                CheckersColor::White => i32::from(7 - row),
            };
            score += progress * 5;
        }
    }

    score
}

fn endgame_mop_up(board: &Board, winner: CheckersColor) -> i32 {
    let loser = winner.opponent();
    let win_mask = board.pieces_of(winner);
    let lose_mask = board.pieces_of(loser);

    let mut bonus = 0_i32;
    let mut l_rem = lose_mask;
    while l_rem != 0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "trailing_zeros() on non-zero u32 is in 0..32"
        )]
        let l_idx = l_rem.trailing_zeros() as u8;
        l_rem &= l_rem - 1;
        let Some(l_sq) = Square::from_index(l_idx) else {
            continue;
        };
        let (lr, lc) = l_sq.row_col();
        // Reward pushing the losing piece away from the center (3.5, 3.5).
        let center_dist = (i32::from(lr) * 2 - 7).abs() + (i32::from(lc) * 2 - 7).abs();
        bonus += center_dist * 4;

        // Reward winning pieces closing Manhattan distance to the losing piece.
        let mut w_rem = win_mask;
        while w_rem != 0 {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "trailing_zeros() on non-zero u32 is in 0..32"
            )]
            let w_idx = w_rem.trailing_zeros() as u8;
            w_rem &= w_rem - 1;
            if let Some(w_sq) = Square::from_index(w_idx) {
                let (wr, wc) = w_sq.row_col();
                let dist =
                    (i32::from(wr) - i32::from(lr)).abs() + (i32::from(wc) - i32::from(lc)).abs();
                bonus += (14 - dist) * 3;
            }
        }
    }
    bonus
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkers::Piece;

    #[test]
    fn time_sliced_search_matches_full_search_identically() {
        let board = Board::starting();
        let mut full = CheckersSearch::new(&board, BotLevel::Hard, 777);
        let full_mv = full.run_to_completion();

        let mut sliced = CheckersSearch::new(&board, BotLevel::Hard, 777);
        let sliced_mv = loop {
            match sliced.step(500) {
                SearchProgress::InProgress => {}
                SearchProgress::Done(mv) => break mv,
            }
        };

        assert!(full_mv.is_some());
        assert_eq!(full_mv, sliced_mv);
    }

    #[test]
    fn hard_ai_captures_hanging_piece_and_returns_legal_move() {
        let mut board = Board::empty(CheckersColor::Black);
        board.set_piece(Square::from_notation(10).unwrap(), Some(Piece::BlackMan));
        board.set_piece(Square::from_notation(14).unwrap(), Some(Piece::WhiteMan));

        let mut search = CheckersSearch::new(&board, BotLevel::Hard, 1);
        let mv = search.run_to_completion().unwrap();
        assert_eq!(mv.notation(), "10x17");
    }
}
