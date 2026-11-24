//! English Draughts move generator, multi-jump logic, crowning, and draw detection.

use std::collections::BTreeMap;

use crate::checkers::board::{Board, CheckersColor, Piece, PositionKey, Square};

/// Half-moves (plies) without a capture or man move before a 40-move draw is declared.
pub const FORTY_MOVE_HALF_MOVES: u16 = 80;

/// Errors returned by Checkers rule operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckersError {
    /// Board string failed validation.
    InvalidBoard,
    /// Action attempted after the game has already finished.
    GameOver,
    /// Submitted move path does not match any legal move.
    IllegalMove,
}

/// Reason a player won a Checkers match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinReason {
    /// Opponent has no pieces or no legal moves remaining.
    NoMoves,
    /// Opponent resigned.
    Resign,
    /// Opponent ran out of time.
    Timeout,
}

/// Reason a Checkers match ended in a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawReason {
    /// Both players agreed to a draw.
    Agreement,
    /// Same position occurred three times.
    ThreefoldRepetition,
    /// 40 moves by each side without a capture or man move.
    FortyMoveRule,
}

/// Terminal outcome of a Checkers match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckersOutcome {
    /// Decisive victory for `winner`.
    Win {
        /// Winning color.
        winner: CheckersColor,
        /// Reason for victory.
        reason: WinReason,
    },
    /// Drawn match.
    Draw {
        /// Draw condition met.
        reason: DrawReason,
    },
}

/// A validated legal move (either a quiet 1-square step or a single/multi-jump capture).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckersMove {
    /// Ordered sequence of squares visited (`[from, to]` or `[from, hop1, hop2, ...]`).
    pub path: Vec<Square>,
    /// Ordered sequence of captured opponent squares (empty for quiet moves).
    pub captured: Vec<Square>,
    /// Whether the moving man was crowned king at the end of this move.
    pub crowned: bool,
}

impl CheckersMove {
    /// Returns `true` if this move captures one or more opponent pieces.
    #[must_use]
    pub fn is_capture(&self) -> bool {
        !self.captured.is_empty()
    }

    /// Formats the move in standard draughts notation (`11-15`, `22x15`, `22x15x8`).
    #[must_use]
    pub fn notation(&self) -> String {
        let sep = if self.is_capture() { "x" } else { "-" };
        let mut out = String::new();
        for (idx, sq) in self.path.iter().enumerate() {
            if idx > 0 {
                out.push_str(sep);
            }
            out.push_str(&sq.notation().to_string());
        }
        out
    }
}

/// Generates all legal moves for `board.turn` according to English Draughts rules.
///
/// Captures are mandatory. When multiple capture sequences exist, all complete
/// capture chains are returned (no maximum-capture restriction). Crowning a man
/// mid-jump immediately terminates the jump sequence.
#[must_use]
pub fn generate_legal_moves(board: &Board) -> Vec<CheckersMove> {
    let mut captures = Vec::new();
    let own_mask = board.pieces_of(board.turn);
    let mut remaining = own_mask;

    while remaining != 0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "trailing_zeros() on non-zero u32 is in 0..32"
        )]
        let idx = remaining.trailing_zeros() as u8;
        remaining &= remaining - 1;
        if let Some(sq) = Square::from_index(idx)
            && let Some(piece) = board.piece_at(sq)
        {
            collect_captures(board, sq, piece, &mut captures);
        }
    }

    if !captures.is_empty() {
        return captures;
    }

    let mut quiet = Vec::new();
    let mut remaining = own_mask;
    while remaining != 0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "trailing_zeros() on non-zero u32 is in 0..32"
        )]
        let idx = remaining.trailing_zeros() as u8;
        remaining &= remaining - 1;
        if let Some(sq) = Square::from_index(idx)
            && let Some(piece) = board.piece_at(sq)
        {
            collect_quiet_moves(board, sq, piece, &mut quiet);
        }
    }
    quiet
}

/// Applies a legal [`CheckersMove`] to `board`, returning the resulting board.
#[must_use]
pub fn apply_move_to_board(board: &Board, mv: &CheckersMove) -> Board {
    let mut next = *board;
    if let (Some(&from), Some(&to)) = (mv.path.first(), mv.path.last())
        && let Some(piece) = next.piece_at(from)
    {
        next.set_piece(from, None);
        for &cap in &mv.captured {
            next.set_piece(cap, None);
        }
        let placed = if mv.crowned {
            match piece.color() {
                CheckersColor::Black => Piece::BlackKing,
                CheckersColor::White => Piece::WhiteKing,
            }
        } else {
            piece
        };
        next.set_piece(to, Some(placed));
        next.turn = next.turn.opponent();
    }
    next
}

fn directions_for_piece(piece: Piece) -> &'static [(i8, i8)] {
    match piece {
        Piece::BlackMan => &[(1, -1), (1, 1)],
        Piece::WhiteMan => &[(-1, -1), (-1, 1)],
        Piece::BlackKing | Piece::WhiteKing => &[(1, -1), (1, 1), (-1, -1), (-1, 1)],
    }
}

fn collect_quiet_moves(board: &Board, from: Square, piece: Piece, out: &mut Vec<CheckersMove>) {
    let occupied = board.occupied();
    let king_row = piece.color().king_row();
    for &(dr, dc) in directions_for_piece(piece) {
        if let Some(to) = from.offset(dr, dc)
            && (occupied & to.bit()) == 0
        {
            let crowned = !piece.is_king() && to.row_col().0 == king_row;
            out.push(CheckersMove {
                path: vec![from, to],
                captured: Vec::new(),
                crowned,
            });
        }
    }
}

fn collect_captures(board: &Board, origin: Square, piece: Piece, out: &mut Vec<CheckersMove>) {
    // Temporarily vacate origin so a king can legally land back on its starting square in a 4-jump loop.
    let occupied_without_origin = board.occupied() & !origin.bit();
    let enemy_mask = board.pieces_of(piece.color().opponent());

    let mut path = vec![origin];
    let mut captured = Vec::new();
    dfs_captures(
        origin,
        piece,
        occupied_without_origin,
        enemy_mask,
        0,
        &mut path,
        &mut captured,
        out,
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "Recursive bitboard DFS keeps temporary masks on the stack with zero heap clones"
)]
fn dfs_captures(
    current: Square,
    piece: Piece,
    occupied_without_origin: u32,
    enemy_mask: u32,
    captured_mask: u32,
    path: &mut Vec<Square>,
    captured: &mut Vec<Square>,
    out: &mut Vec<CheckersMove>,
) {
    let king_row = piece.color().king_row();
    let mut extended = false;

    for &(dr, dc) in directions_for_piece(piece) {
        let Some(mid) = current.offset(dr, dc) else {
            continue;
        };
        let mid_bit = mid.bit();
        if (enemy_mask & mid_bit) == 0 || (captured_mask & mid_bit) != 0 {
            continue;
        }
        let Some(land) = current.offset(dr * 2, dc * 2) else {
            continue;
        };
        let land_bit = land.bit();
        if (occupied_without_origin & land_bit) != 0 {
            continue;
        }

        extended = true;
        path.push(land);
        captured.push(mid);

        let crowned = !piece.is_king() && land.row_col().0 == king_row;
        if crowned {
            // English Draughts rule C-4: crowning immediately ends the turn.
            out.push(CheckersMove {
                path: path.clone(),
                captured: captured.clone(),
                crowned: true,
            });
        } else {
            dfs_captures(
                land,
                piece,
                occupied_without_origin,
                enemy_mask,
                captured_mask | mid_bit,
                path,
                captured,
                out,
            );
        }

        path.pop();
        captured.pop();
    }

    if !extended && !captured.is_empty() {
        out.push(CheckersMove {
            path: path.clone(),
            captured: captured.clone(),
            crowned: false,
        });
    }
}

/// Full stateful Checkers match enforcing rules, clocks, repetition, and draw conditions.
#[derive(Debug, Clone)]
pub struct CheckersGame {
    board: Board,
    legal_moves: Vec<CheckersMove>,
    move_history: Vec<String>,
    last_move: Option<CheckersMove>,
    half_move_clock: u16,
    position_counts: BTreeMap<PositionKey, u8>,
    outcome: Option<CheckersOutcome>,
}

impl Default for CheckersGame {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckersGame {
    /// Starts a standard English Draughts match.
    #[must_use]
    pub fn new() -> Self {
        Self::from_board(Board::starting())
    }

    /// Starts a match from an arbitrary valid [`Board`].
    #[must_use]
    pub fn from_board(board: Board) -> Self {
        let legal_moves = generate_legal_moves(&board);
        let outcome = if legal_moves.is_empty() {
            Some(CheckersOutcome::Win {
                winner: board.turn.opponent(),
                reason: WinReason::NoMoves,
            })
        } else {
            None
        };
        let mut position_counts = BTreeMap::new();
        position_counts.insert(board.position_key(), 1);
        Self {
            board,
            legal_moves,
            move_history: Vec::new(),
            last_move: None,
            half_move_clock: 0,
            position_counts,
            outcome,
        }
    }

    /// Current board state.
    #[must_use]
    pub const fn board(&self) -> &Board {
        &self.board
    }

    /// Legal moves available to the active player (empty once the match is over).
    #[must_use]
    pub fn legal_moves(&self) -> &[CheckersMove] {
        if self.outcome.is_some() {
            &[]
        } else {
            &self.legal_moves
        }
    }

    /// Returns `true` if the active player must execute a capture move.
    #[must_use]
    pub fn has_mandatory_capture(&self) -> bool {
        self.outcome.is_none()
            && self
                .legal_moves
                .first()
                .is_some_and(CheckersMove::is_capture)
    }

    /// Move history in standard notation.
    #[must_use]
    pub fn move_history(&self) -> &[String] {
        &self.move_history
    }

    /// Most recently played move, if any.
    #[must_use]
    pub const fn last_move(&self) -> Option<&CheckersMove> {
        self.last_move.as_ref()
    }

    /// Terminal outcome, or `None` while the match is in progress.
    #[must_use]
    pub const fn outcome(&self) -> Option<CheckersOutcome> {
        self.outcome
    }

    /// Returns `(captured_by_black, captured_by_white)` relative to standard 12-piece start.
    #[must_use]
    pub const fn captured_counts(&self) -> (u8, u8) {
        let (black_left, white_left) = self.board.piece_counts();
        (
            12_u8.saturating_sub(white_left),
            12_u8.saturating_sub(black_left),
        )
    }

    /// Applies a move identified by its ordered `path` of squares.
    ///
    /// # Errors
    /// Returns [`CheckersError::GameOver`] if already finished or [`CheckersError::IllegalMove`]
    /// if `path` does not match any legal move.
    pub fn apply_path(&mut self, path: &[Square]) -> Result<CheckersMove, CheckersError> {
        if self.outcome.is_some() {
            return Err(CheckersError::GameOver);
        }
        let Some(matched) = self
            .legal_moves
            .iter()
            .find(|mv| mv.path.as_slice() == path)
            .cloned()
        else {
            return Err(CheckersError::IllegalMove);
        };

        let moved_man = matched
            .path
            .first()
            .and_then(|&sq| self.board.piece_at(sq))
            .is_some_and(|p| !p.is_king());

        self.board = apply_move_to_board(&self.board, &matched);
        self.move_history.push(matched.notation());
        self.last_move = Some(matched.clone());

        if matched.is_capture() || moved_man {
            self.half_move_clock = 0;
        } else {
            self.half_move_clock = self.half_move_clock.saturating_add(1);
        }

        let key = self.board.position_key();
        let count = self.position_counts.entry(key).or_insert(0);
        *count = count.saturating_add(1);
        let repetition_count = *count;

        self.legal_moves = generate_legal_moves(&self.board);

        if self.legal_moves.is_empty() {
            self.outcome = Some(CheckersOutcome::Win {
                winner: self.board.turn.opponent(),
                reason: WinReason::NoMoves,
            });
        } else if repetition_count >= 3 {
            self.outcome = Some(CheckersOutcome::Draw {
                reason: DrawReason::ThreefoldRepetition,
            });
        } else if self.half_move_clock >= FORTY_MOVE_HALF_MOVES {
            self.outcome = Some(CheckersOutcome::Draw {
                reason: DrawReason::FortyMoveRule,
            });
        }

        Ok(matched)
    }

    /// Ends the match by resignation of `loser`.
    ///
    /// # Errors
    /// Returns [`CheckersError::GameOver`] if already finished.
    pub fn resign(&mut self, loser: CheckersColor) -> Result<(), CheckersError> {
        if self.outcome.is_some() {
            return Err(CheckersError::GameOver);
        }
        self.outcome = Some(CheckersOutcome::Win {
            winner: loser.opponent(),
            reason: WinReason::Resign,
        });
        Ok(())
    }

    /// Ends the match because `loser` ran out of time.
    ///
    /// # Errors
    /// Returns [`CheckersError::GameOver`] if already finished.
    pub fn timeout(&mut self, loser: CheckersColor) -> Result<(), CheckersError> {
        if self.outcome.is_some() {
            return Err(CheckersError::GameOver);
        }
        self.outcome = Some(CheckersOutcome::Win {
            winner: loser.opponent(),
            reason: WinReason::Timeout,
        });
        Ok(())
    }

    /// Ends the match as a draw by mutual agreement.
    ///
    /// # Errors
    /// Returns [`CheckersError::GameOver`] if already finished.
    pub fn agree_draw(&mut self) -> Result<(), CheckersError> {
        if self.outcome.is_some() {
            return Err(CheckersError::GameOver);
        }
        self.outcome = Some(CheckersOutcome::Draw {
            reason: DrawReason::Agreement,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use proptest::prelude::*;

    use super::*;

    fn perft(board: &Board, depth: u8) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = generate_legal_moves(board);
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0_u64;
        for mv in &moves {
            let next = apply_move_to_board(board, mv);
            nodes += perft(&next, depth - 1);
        }
        nodes
    }

    #[test]
    fn english_draughts_perft_matches_known_values() {
        let start = Board::starting();
        assert_eq!(perft(&start, 1), 7);
        assert_eq!(perft(&start, 2), 49);
        assert_eq!(perft(&start, 3), 302);
        assert_eq!(perft(&start, 4), 1_469);
        assert_eq!(perft(&start, 5), 7_361);
        assert_eq!(perft(&start, 6), 36_768);
    }

    #[test]
    fn mandatory_multi_jump_is_enforced_and_formatted() {
        let mut board = Board::empty(CheckersColor::Black);
        board.set_piece(Square::from_notation(10).unwrap(), Some(Piece::BlackMan));
        board.set_piece(Square::from_notation(14).unwrap(), Some(Piece::WhiteMan));
        board.set_piece(Square::from_notation(22).unwrap(), Some(Piece::WhiteMan));
        // Add another Black man with a quiet move to confirm captures override quiet moves.
        board.set_piece(Square::from_notation(1).unwrap(), Some(Piece::BlackMan));

        let moves = generate_legal_moves(&board);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].notation(), "10x17x26");
        assert_eq!(moves[0].captured.len(), 2);
        assert!(!moves[0].crowned);
    }

    #[test]
    fn crowning_mid_jump_immediately_ends_turn() {
        // Black man on 22 jumps white man on 26 -> lands on 31 (row 7, king row).
        // Even though another white man sits on 27 (jumpable by a king to 24),
        // English Draughts rule C-4 terminates the move immediately upon crowning.
        let mut board = Board::empty(CheckersColor::Black);
        board.set_piece(Square::from_notation(22).unwrap(), Some(Piece::BlackMan));
        board.set_piece(Square::from_notation(26).unwrap(), Some(Piece::WhiteMan));
        board.set_piece(Square::from_notation(27).unwrap(), Some(Piece::WhiteMan));

        let moves = generate_legal_moves(&board);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].notation(), "22x31");
        assert!(moves[0].crowned);
        let next = apply_move_to_board(&board, &moves[0]);
        assert_eq!(
            next.piece_at(Square::from_notation(31).unwrap()),
            Some(Piece::BlackKing)
        );
        assert_eq!(
            next.piece_at(Square::from_notation(27).unwrap()),
            Some(Piece::WhiteMan)
        );
    }

    #[test]
    fn threefold_repetition_declares_draw() {
        let mut board = Board::empty(CheckersColor::Black);
        board.set_piece(Square::from_notation(1).unwrap(), Some(Piece::BlackKing));
        board.set_piece(Square::from_notation(32).unwrap(), Some(Piece::WhiteKing));
        let mut game = CheckersGame::from_board(board);

        let black_out = [
            Square::from_notation(1).unwrap(),
            Square::from_notation(5).unwrap(),
        ];
        let black_back = [
            Square::from_notation(5).unwrap(),
            Square::from_notation(1).unwrap(),
        ];
        let white_out = [
            Square::from_notation(32).unwrap(),
            Square::from_notation(28).unwrap(),
        ];
        let white_back = [
            Square::from_notation(28).unwrap(),
            Square::from_notation(32).unwrap(),
        ];

        // Cycle 1 -> position seen twice
        game.apply_path(&black_out).unwrap();
        game.apply_path(&white_out).unwrap();
        game.apply_path(&black_back).unwrap();
        game.apply_path(&white_back).unwrap();
        assert!(game.outcome().is_none());

        // Cycle 2 -> position seen 3rd time -> draw
        game.apply_path(&black_out).unwrap();
        game.apply_path(&white_out).unwrap();
        game.apply_path(&black_back).unwrap();
        game.apply_path(&white_back).unwrap();
        assert_eq!(
            game.outcome(),
            Some(CheckersOutcome::Draw {
                reason: DrawReason::ThreefoldRepetition,
            })
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    proptest! {
        #[test]
        fn random_playouts_preserve_board_invariants(choices in proptest::collection::vec(any::<usize>(), 1..60)) {
            let mut game = CheckersGame::new();
            for choice in choices {
                let moves = game.legal_moves();
                if moves.is_empty() {
                    break;
                }
                let path = moves[choice % moves.len()].path.clone();
                let _ = game.apply_path(&path).unwrap();
                prop_assert!(game.board().is_valid());
                let (b, w) = game.board().piece_counts();
                prop_assert!(b <= 12 && w <= 12);
            }
        }
    }
}
