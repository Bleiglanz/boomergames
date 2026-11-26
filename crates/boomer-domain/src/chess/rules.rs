//! Complete FIDE Chess move generation, SAN formatting, and match rule state machine.

use std::collections::HashMap;

use crate::chess::board::{
    CastlingRights, ChessColor, ChessPiece, ChessPieceKind, ChessPosition, ChessPositionKey,
    ChessSquare,
};

/// A single legal Chess move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChessMove {
    /// Origin square (`0 = a1` .. `63 = h8`).
    pub from: ChessSquare,
    /// Destination square (`0 = a1` .. `63 = h8`).
    pub to: ChessSquare,
    /// Promotion piece kind if a pawn reaches the last rank.
    pub promotion: Option<ChessPieceKind>,
    /// `true` if this move captures an opponent piece (including en passant).
    pub is_capture: bool,
    /// `true` if this move is an en passant capture.
    pub is_en_passant: bool,
    /// `true` if this move is kingside or queenside castling.
    pub is_castle: bool,
}

impl ChessMove {
    /// Formats this move in UCI coordinate notation (`e2e4`, `e7e8q`).
    #[must_use]
    pub fn to_uci(self) -> String {
        let mut s = String::with_capacity(5);
        s.push(self.from.file_char());
        s.push(self.from.rank_char());
        s.push(self.to.file_char());
        s.push(self.to.rank_char());
        if let Some(promo) = self.promotion.and_then(ChessPieceKind::promotion_char) {
            s.push(promo);
        }
        s
    }

    /// Parses a UCI move string (`e2e4`, `a7a8n`) into `(from, to, promotion)`.
    #[must_use]
    pub fn parse_uci(uci: &str) -> Option<(ChessSquare, ChessSquare, Option<ChessPieceKind>)> {
        let trimmed = uci.trim();
        if trimmed.len() != 4 && trimmed.len() != 5 {
            return None;
        }
        let from = ChessSquare::from_algebraic(trimmed.get(0..2)?)?;
        let to = ChessSquare::from_algebraic(trimmed.get(2..4)?)?;
        let promotion = if trimmed.len() == 5 {
            let ch = trimmed.chars().nth(4)?;
            Some(ChessPieceKind::from_promotion_char(ch)?)
        } else {
            None
        };
        Some((from, to, promotion))
    }
}

/// Reason a Chess match ended decisively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChessWinReason {
    /// Opponent king is checkmated.
    Checkmate,
    /// Opponent resigned.
    Resign,
    /// Opponent ran out of time.
    Timeout,
}

/// Reason a Chess match ended in a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChessDrawReason {
    /// Side to move has no legal moves and is not in check.
    Stalemate,
    /// Mutual draw agreement.
    Agreement,
    /// Same position occurred three times.
    ThreefoldRepetition,
    /// 50 moves per side (100 half-moves) without pawn move or capture.
    FiftyMoveRule,
    /// Insufficient mating material on the board.
    InsufficientMaterial,
}

/// Terminal outcome of a Chess match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChessOutcome {
    /// Decisive win.
    Win {
        /// Winning color.
        winner: ChessColor,
        /// Win reason.
        reason: ChessWinReason,
    },
    /// Drawn game.
    Draw {
        /// Draw reason.
        reason: ChessDrawReason,
    },
}

/// Errors returned when attempting an illegal Chess action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChessError {
    /// Match has already ended.
    GameOver,
    /// Submitted move is not legal in the current position.
    IllegalMove,
}

/// Applies `mv` to a copy of `pos` without checking king safety.
#[must_use]
pub fn apply_move_unchecked(pos: &ChessPosition, mv: ChessMove) -> ChessPosition {
    let mut next = pos.clone();
    let Some(moving) = next.piece_at(mv.from) else {
        return next;
    };

    // Remove piece from origin.
    next.set_piece(mv.from, None);

    // Handle en passant capture (captured pawn sits on from-rank, to-file).
    if mv.is_en_passant
        && let Some(cap_sq) = ChessSquare::from_file_rank(mv.to.file(), mv.from.rank())
    {
        next.set_piece(cap_sq, None);
    }

    // Handle castling rook movement.
    if mv.is_castle {
        let rank = mv.from.rank();
        let (rook_from_file, rook_to_file) = if mv.to.file() == 6 { (7, 5) } else { (0, 3) };
        if let (Some(rf), Some(rt)) = (
            ChessSquare::from_file_rank(rook_from_file, rank),
            ChessSquare::from_file_rank(rook_to_file, rank),
        ) {
            let rook = next.piece_at(rf);
            next.set_piece(rf, None);
            next.set_piece(rt, rook);
        }
    }

    // Place moved (or promoted) piece on destination.
    let final_kind = mv.promotion.unwrap_or(moving.kind);
    next.set_piece(mv.to, Some(ChessPiece::new(moving.color, final_kind)));

    // Update castling rights whenever king or corner rooks move or are captured.
    update_castling_for_square(&mut next.castling, mv.from);
    update_castling_for_square(&mut next.castling, mv.to);

    // Set en passant square only on double pawn push.
    next.en_passant = None;
    if moving.kind == ChessPieceKind::Pawn {
        let rank_diff = mv.to.rank().abs_diff(mv.from.rank());
        if rank_diff == 2 {
            let mid_rank = u8::midpoint(mv.from.rank(), mv.to.rank());
            next.en_passant = ChessSquare::from_file_rank(mv.from.file(), mid_rank);
        }
    }

    // Update clocks.
    if moving.kind == ChessPieceKind::Pawn || mv.is_capture {
        next.halfmove_clock = 0;
    } else {
        next.halfmove_clock = next.halfmove_clock.saturating_add(1);
    }
    if pos.turn == ChessColor::Black {
        next.fullmove_number = next.fullmove_number.saturating_add(1);
    }
    next.turn = pos.turn.opponent();
    next
}

const fn update_castling_for_square(rights: &mut CastlingRights, sq: ChessSquare) {
    match sq.raw() {
        0 => rights.clear(CastlingRights::WHITE_QUEEN), // a1
        4 => rights.clear(CastlingRights::WHITE_KING | CastlingRights::WHITE_QUEEN), // e1
        7 => rights.clear(CastlingRights::WHITE_KING),  // h1
        56 => rights.clear(CastlingRights::BLACK_QUEEN), // a8
        60 => rights.clear(CastlingRights::BLACK_KING | CastlingRights::BLACK_QUEEN), // e8
        63 => rights.clear(CastlingRights::BLACK_KING), // h8
        _ => {}
    }
}

/// Generates all strictly legal Chess moves for `pos.turn`.
#[must_use]
pub fn generate_legal_moves(pos: &ChessPosition) -> Vec<ChessMove> {
    let mut pseudo = Vec::with_capacity(48);
    let side = pos.turn;

    for idx in 0..64_u8 {
        let Some(from) = ChessSquare::from_index(idx) else {
            continue;
        };
        let Some(piece) = pos.piece_at(from) else {
            continue;
        };
        if piece.color != side {
            continue;
        }
        match piece.kind {
            ChessPieceKind::Pawn => gen_pawn_moves(pos, from, side, &mut pseudo),
            ChessPieceKind::Knight => gen_step_moves(
                pos,
                from,
                side,
                &[
                    (-2, -1),
                    (-2, 1),
                    (-1, -2),
                    (-1, 2),
                    (1, -2),
                    (1, 2),
                    (2, -1),
                    (2, 1),
                ],
                &mut pseudo,
            ),
            ChessPieceKind::Bishop => {
                gen_slider_moves(
                    pos,
                    from,
                    side,
                    &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
                    &mut pseudo,
                );
            }
            ChessPieceKind::Rook => {
                gen_slider_moves(
                    pos,
                    from,
                    side,
                    &[(-1, 0), (1, 0), (0, -1), (0, 1)],
                    &mut pseudo,
                );
            }
            ChessPieceKind::Queen => {
                gen_slider_moves(
                    pos,
                    from,
                    side,
                    &[
                        (-1, -1),
                        (-1, 1),
                        (1, -1),
                        (1, 1),
                        (-1, 0),
                        (1, 0),
                        (0, -1),
                        (0, 1),
                    ],
                    &mut pseudo,
                );
            }
            ChessPieceKind::King => {
                gen_step_moves(
                    pos,
                    from,
                    side,
                    &[
                        (-1, -1),
                        (-1, 0),
                        (-1, 1),
                        (0, -1),
                        (0, 1),
                        (1, -1),
                        (1, 0),
                        (1, 1),
                    ],
                    &mut pseudo,
                );
                gen_castling_moves(pos, from, side, &mut pseudo);
            }
        }
    }

    pseudo
        .into_iter()
        .filter(|&mv| !apply_move_unchecked(pos, mv).is_in_check(side))
        .collect()
}

fn gen_pawn_moves(
    pos: &ChessPosition,
    from: ChessSquare,
    side: ChessColor,
    out: &mut Vec<ChessMove>,
) {
    const PROMOS: [ChessPieceKind; 4] = [
        ChessPieceKind::Queen,
        ChessPieceKind::Rook,
        ChessPieceKind::Bishop,
        ChessPieceKind::Knight,
    ];
    let (dir, start_rank, promo_rank): (i8, u8, u8) = match side {
        ChessColor::White => (1, 1, 7),
        ChessColor::Black => (-1, 6, 0),
    };

    // Forward pushes
    if let Some(one) = from.offset(0, dir)
        && pos.piece_at(one).is_none()
    {
        if one.rank() == promo_rank {
            for kind in PROMOS {
                out.push(ChessMove {
                    from,
                    to: one,
                    promotion: Some(kind),
                    is_capture: false,
                    is_en_passant: false,
                    is_castle: false,
                });
            }
        } else {
            out.push(ChessMove {
                from,
                to: one,
                promotion: None,
                is_capture: false,
                is_en_passant: false,
                is_castle: false,
            });
            if from.rank() == start_rank
                && let Some(two) = one.offset(0, dir)
                && pos.piece_at(two).is_none()
            {
                out.push(ChessMove {
                    from,
                    to: two,
                    promotion: None,
                    is_capture: false,
                    is_en_passant: false,
                    is_castle: false,
                });
            }
        }
    }

    // Diagonal captures + en passant
    for df in [-1_i8, 1] {
        let Some(target) = from.offset(df, dir) else {
            continue;
        };
        if let Some(occ) = pos.piece_at(target)
            && occ.color != side
        {
            if target.rank() == promo_rank {
                for kind in PROMOS {
                    out.push(ChessMove {
                        from,
                        to: target,
                        promotion: Some(kind),
                        is_capture: true,
                        is_en_passant: false,
                        is_castle: false,
                    });
                }
            } else {
                out.push(ChessMove {
                    from,
                    to: target,
                    promotion: None,
                    is_capture: true,
                    is_en_passant: false,
                    is_castle: false,
                });
            }
        } else if pos.en_passant == Some(target) {
            out.push(ChessMove {
                from,
                to: target,
                promotion: None,
                is_capture: true,
                is_en_passant: true,
                is_castle: false,
            });
        }
    }
}

fn gen_step_moves(
    pos: &ChessPosition,
    from: ChessSquare,
    side: ChessColor,
    offsets: &[(i8, i8)],
    out: &mut Vec<ChessMove>,
) {
    for &(df, dr) in offsets {
        let Some(to) = from.offset(df, dr) else {
            continue;
        };
        match pos.piece_at(to) {
            None => out.push(ChessMove {
                from,
                to,
                promotion: None,
                is_capture: false,
                is_en_passant: false,
                is_castle: false,
            }),
            Some(occ) if occ.color != side => out.push(ChessMove {
                from,
                to,
                promotion: None,
                is_capture: true,
                is_en_passant: false,
                is_castle: false,
            }),
            Some(_) => {}
        }
    }
}

fn gen_slider_moves(
    pos: &ChessPosition,
    from: ChessSquare,
    side: ChessColor,
    dirs: &[(i8, i8)],
    out: &mut Vec<ChessMove>,
) {
    for &(df, dr) in dirs {
        let mut cur = from;
        while let Some(to) = cur.offset(df, dr) {
            cur = to;
            match pos.piece_at(to) {
                None => out.push(ChessMove {
                    from,
                    to,
                    promotion: None,
                    is_capture: false,
                    is_en_passant: false,
                    is_castle: false,
                }),
                Some(occ) => {
                    if occ.color != side {
                        out.push(ChessMove {
                            from,
                            to,
                            promotion: None,
                            is_capture: true,
                            is_en_passant: false,
                            is_castle: false,
                        });
                    }
                    break;
                }
            }
        }
    }
}

fn gen_castling_moves(
    pos: &ChessPosition,
    from: ChessSquare,
    side: ChessColor,
    out: &mut Vec<ChessMove>,
) {
    let (rank, king_flag, queen_flag) = match side {
        ChessColor::White => (
            0_u8,
            CastlingRights::WHITE_KING,
            CastlingRights::WHITE_QUEEN,
        ),
        ChessColor::Black => (
            7_u8,
            CastlingRights::BLACK_KING,
            CastlingRights::BLACK_QUEEN,
        ),
    };
    let Some(expected_king) = ChessSquare::from_file_rank(4, rank) else {
        return;
    };
    if from != expected_king {
        return;
    }
    let opp = side.opponent();
    if pos.is_square_attacked(from, opp) {
        return;
    }

    // Kingside O-O (f & g empty and not attacked)
    if pos.castling.has(king_flag)
        && let (Some(f_sq), Some(g_sq), Some(h_sq)) = (
            ChessSquare::from_file_rank(5, rank),
            ChessSquare::from_file_rank(6, rank),
            ChessSquare::from_file_rank(7, rank),
        )
        && pos.piece_at(f_sq).is_none()
        && pos.piece_at(g_sq).is_none()
        && pos.piece_at(h_sq) == Some(ChessPiece::new(side, ChessPieceKind::Rook))
        && !pos.is_square_attacked(f_sq, opp)
        && !pos.is_square_attacked(g_sq, opp)
    {
        out.push(ChessMove {
            from,
            to: g_sq,
            promotion: None,
            is_capture: false,
            is_en_passant: false,
            is_castle: true,
        });
    }

    // Queenside O-O-O (b, c, d empty; c & d not attacked)
    if pos.castling.has(queen_flag)
        && let (Some(a_sq), Some(b_sq), Some(c_sq), Some(d_sq)) = (
            ChessSquare::from_file_rank(0, rank),
            ChessSquare::from_file_rank(1, rank),
            ChessSquare::from_file_rank(2, rank),
            ChessSquare::from_file_rank(3, rank),
        )
        && pos.piece_at(b_sq).is_none()
        && pos.piece_at(c_sq).is_none()
        && pos.piece_at(d_sq).is_none()
        && pos.piece_at(a_sq) == Some(ChessPiece::new(side, ChessPieceKind::Rook))
        && !pos.is_square_attacked(d_sq, opp)
        && !pos.is_square_attacked(c_sq, opp)
    {
        out.push(ChessMove {
            from,
            to: c_sq,
            promotion: None,
            is_capture: false,
            is_en_passant: false,
            is_castle: true,
        });
    }
}

/// Formats `mv` in Standard Algebraic Notation (SAN), including disambiguation and `+`/`#`.
#[must_use]
pub fn format_san(
    before: &ChessPosition,
    legal_before: &[ChessMove],
    mv: ChessMove,
    after: &ChessPosition,
    after_has_moves: bool,
) -> String {
    let mut san = String::with_capacity(8);
    if mv.is_castle {
        san.push_str(if mv.to.file() == 6 { "O-O" } else { "O-O-O" });
    } else if let Some(piece) = before.piece_at(mv.from) {
        if let Some(letter) = piece.kind.san_letter() {
            san.push(letter);
            // Disambiguate if another same-kind piece could legally reach `mv.to`.
            let mut same_file = false;
            let mut same_rank = false;
            let mut ambiguous = false;
            for other in legal_before {
                if other.from != mv.from
                    && other.to == mv.to
                    && before.piece_at(other.from) == Some(piece)
                {
                    ambiguous = true;
                    if other.from.file() == mv.from.file() {
                        same_file = true;
                    }
                    if other.from.rank() == mv.from.rank() {
                        same_rank = true;
                    }
                }
            }
            if ambiguous {
                if !same_file {
                    san.push(mv.from.file_char());
                } else if !same_rank {
                    san.push(mv.from.rank_char());
                } else {
                    san.push(mv.from.file_char());
                    san.push(mv.from.rank_char());
                }
            }
        } else if mv.is_capture {
            san.push(mv.from.file_char());
        }

        if mv.is_capture {
            san.push('x');
        }
        san.push(mv.to.file_char());
        san.push(mv.to.rank_char());

        if let Some(promo) = mv.promotion.and_then(ChessPieceKind::san_letter) {
            san.push('=');
            san.push(promo);
        }
    }

    if after.is_in_check(after.turn) {
        san.push(if after_has_moves { '+' } else { '#' });
    }
    san
}

/// Complete stateful Chess match engine tracking legal moves, SAN history, captures, and draw rules.
#[derive(Debug, Clone)]
pub struct ChessGame {
    position: ChessPosition,
    legal_moves: Vec<ChessMove>,
    move_history: Vec<String>,
    last_move: Option<ChessMove>,
    captured_by_white: Vec<ChessPieceKind>,
    captured_by_black: Vec<ChessPieceKind>,
    repetitions: HashMap<ChessPositionKey, u8>,
    outcome: Option<ChessOutcome>,
}

impl Default for ChessGame {
    fn default() -> Self {
        Self::new()
    }
}

impl ChessGame {
    /// Starts a fresh standard FIDE Chess match.
    #[must_use]
    pub fn new() -> Self {
        let position = ChessPosition::starting();
        let legal_moves = generate_legal_moves(&position);
        let mut repetitions = HashMap::new();
        repetitions.insert(position.position_key(), 1);
        Self {
            position,
            legal_moves,
            move_history: Vec::new(),
            last_move: None,
            captured_by_white: Vec::new(),
            captured_by_black: Vec::new(),
            repetitions,
            outcome: None,
        }
    }

    /// Current board position.
    #[must_use]
    pub const fn position(&self) -> &ChessPosition {
        &self.position
    }

    /// All legal moves in the current position (empty if the game ended).
    #[must_use]
    pub fn legal_moves(&self) -> &[ChessMove] {
        &self.legal_moves
    }

    /// Recorded SAN move history.
    #[must_use]
    pub fn move_history(&self) -> &[String] {
        &self.move_history
    }

    /// Last played move, if any.
    #[must_use]
    pub const fn last_move(&self) -> Option<ChessMove> {
        self.last_move
    }

    /// Opponent pieces captured by White.
    #[must_use]
    pub fn captured_by_white(&self) -> &[ChessPieceKind] {
        &self.captured_by_white
    }

    /// Opponent pieces captured by Black.
    #[must_use]
    pub fn captured_by_black(&self) -> &[ChessPieceKind] {
        &self.captured_by_black
    }

    /// Terminal match outcome, if finished.
    #[must_use]
    pub const fn outcome(&self) -> Option<ChessOutcome> {
        self.outcome
    }

    /// Returns `true` if the active side's king is currently in check.
    #[must_use]
    pub fn in_check(&self) -> bool {
        self.position.is_in_check(self.position.turn)
    }

    /// Applies a move specified in UCI notation (e.g. `"e2e4"`, `"e7e8q"`).
    ///
    /// # Errors
    /// Returns [`ChessError::GameOver`] or [`ChessError::IllegalMove`].
    pub fn apply_uci(&mut self, uci: &str) -> Result<ChessMove, ChessError> {
        if self.outcome.is_some() {
            return Err(ChessError::GameOver);
        }
        let (from, to, promo) = ChessMove::parse_uci(uci).ok_or(ChessError::IllegalMove)?;
        let mv = self
            .legal_moves
            .iter()
            .copied()
            .find(|m| m.from == from && m.to == to && m.promotion == promo)
            .ok_or(ChessError::IllegalMove)?;

        let captured_kind = if mv.is_en_passant {
            Some(ChessPieceKind::Pawn)
        } else if mv.is_capture {
            self.position.piece_at(mv.to).map(|p| p.kind)
        } else {
            None
        };

        let mover = self.position.turn;
        let next_pos = apply_move_unchecked(&self.position, mv);
        let next_legal = generate_legal_moves(&next_pos);
        let san = format_san(
            &self.position,
            &self.legal_moves,
            mv,
            &next_pos,
            !next_legal.is_empty(),
        );

        if let Some(cap) = captured_kind {
            match mover {
                ChessColor::White => self.captured_by_white.push(cap),
                ChessColor::Black => self.captured_by_black.push(cap),
            }
        }

        self.position = next_pos;
        self.move_history.push(san);
        self.last_move = Some(mv);

        let key = self.position.position_key();
        let count = self.repetitions.entry(key).or_insert(0);
        *count = count.saturating_add(1);
        let rep_count = *count;

        if next_legal.is_empty() {
            if self.position.is_in_check(self.position.turn) {
                self.outcome = Some(ChessOutcome::Win {
                    winner: mover,
                    reason: ChessWinReason::Checkmate,
                });
            } else {
                self.outcome = Some(ChessOutcome::Draw {
                    reason: ChessDrawReason::Stalemate,
                });
            }
            self.legal_moves.clear();
        } else if self.position.is_insufficient_material() {
            self.outcome = Some(ChessOutcome::Draw {
                reason: ChessDrawReason::InsufficientMaterial,
            });
            self.legal_moves.clear();
        } else if rep_count >= 3 {
            self.outcome = Some(ChessOutcome::Draw {
                reason: ChessDrawReason::ThreefoldRepetition,
            });
            self.legal_moves.clear();
        } else if self.position.halfmove_clock >= 100 {
            self.outcome = Some(ChessOutcome::Draw {
                reason: ChessDrawReason::FiftyMoveRule,
            });
            self.legal_moves.clear();
        } else {
            self.legal_moves = next_legal;
        }

        Ok(mv)
    }

    /// Declares resignation by `loser`.
    ///
    /// # Errors
    /// Returns [`ChessError::GameOver`] if the match already ended.
    pub fn resign(&mut self, loser: ChessColor) -> Result<(), ChessError> {
        if self.outcome.is_some() {
            return Err(ChessError::GameOver);
        }
        self.outcome = Some(ChessOutcome::Win {
            winner: loser.opponent(),
            reason: ChessWinReason::Resign,
        });
        self.legal_moves.clear();
        Ok(())
    }

    /// Declares loss on time by `flagged`.
    ///
    /// # Errors
    /// Returns [`ChessError::GameOver`] if the match already ended.
    pub fn timeout(&mut self, flagged: ChessColor) -> Result<(), ChessError> {
        if self.outcome.is_some() {
            return Err(ChessError::GameOver);
        }
        self.outcome = Some(ChessOutcome::Win {
            winner: flagged.opponent(),
            reason: ChessWinReason::Timeout,
        });
        self.legal_moves.clear();
        Ok(())
    }

    /// Declares a draw by mutual agreement.
    ///
    /// # Errors
    /// Returns [`ChessError::GameOver`] if the match already ended.
    pub fn agree_draw(&mut self) -> Result<(), ChessError> {
        if self.outcome.is_some() {
            return Err(ChessError::GameOver);
        }
        self.outcome = Some(ChessOutcome::Draw {
            reason: ChessDrawReason::Agreement,
        });
        self.legal_moves.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perft(pos: &ChessPosition, depth: u8) -> u64 {
        let moves = generate_legal_moves(pos);
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0_u64;
        for mv in moves {
            let next = apply_move_unchecked(pos, mv);
            nodes += perft(&next, depth - 1);
        }
        nodes
    }

    #[test]
    fn starting_position_perft_matches_fide_reference() {
        let pos = ChessPosition::starting();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(perft(&pos, 1), 20);
        assert_eq!(perft(&pos, 2), 400);
        assert_eq!(perft(&pos, 3), 8_902);
    }

    #[test]
    fn scholars_mate_checkmates_and_formats_san() {
        let mut g = ChessGame::new();
        for uci in ["e2e4", "e7e5", "d1h5", "b8c6", "f1c4", "g8f6", "h5f7"] {
            g.apply_uci(uci).unwrap();
        }
        assert_eq!(
            g.outcome(),
            Some(ChessOutcome::Win {
                winner: ChessColor::White,
                reason: ChessWinReason::Checkmate,
            })
        );
        assert_eq!(g.move_history().last().map(String::as_str), Some("Qxf7#"));
    }

    #[test]
    fn castling_and_en_passant_work_legally() {
        let mut g = ChessGame::new();
        for uci in ["e2e4", "a7a6", "e4e5", "d7d5", "e5d6"] {
            let mv = g.apply_uci(uci).unwrap();
            if uci == "e5d6" {
                assert!(mv.is_en_passant);
                assert!(mv.is_capture);
            }
        }
        assert_eq!(g.captured_by_white(), &[ChessPieceKind::Pawn]);

        // Kingside castling
        let mut cg = ChessGame::new();
        for uci in ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1"] {
            cg.apply_uci(uci).unwrap();
        }
        assert_eq!(cg.move_history().last().map(String::as_str), Some("O-O"));
    }

    #[test]
    fn threefold_repetition_declares_draw() {
        let mut g = ChessGame::new();
        for uci in [
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ] {
            g.apply_uci(uci).unwrap();
        }
        assert_eq!(
            g.outcome(),
            Some(ChessOutcome::Draw {
                reason: ChessDrawReason::ThreefoldRepetition,
            })
        );
    }
}
