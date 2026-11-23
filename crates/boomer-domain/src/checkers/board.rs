//! 32-square bitboard representation for English Draughts (8x8 dark squares `1..=32`).

use crate::checkers::rules::CheckersError;

/// Validated dark square index `0..32` corresponding to standard draughts notation `1..=32`.
///
/// Row `0` (top, Black home side) holds squares `1..=4` on columns `1, 3, 5, 7`.
/// Row `7` (bottom, White home side) holds squares `29..=32` on columns `0, 2, 4, 6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Square(u8);

impl Square {
    /// Total playable dark squares on an 8x8 checkers board.
    pub const COUNT: u8 = 32;

    /// Creates a `Square` from a 0-based index (`0..32`).
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        if index < Self::COUNT {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Creates a `Square` from standard draughts notation (`1..=32`).
    #[must_use]
    pub const fn from_notation(notation: u8) -> Option<Self> {
        if notation >= 1 && notation <= Self::COUNT {
            Some(Self(notation - 1))
        } else {
            None
        }
    }

    /// Creates a `Square` from 0-based `(row, col)` if it is an in-bounds dark square.
    #[must_use]
    pub const fn from_row_col(row: u8, col: u8) -> Option<Self> {
        if row >= 8 || col >= 8 {
            return None;
        }
        // Dark squares satisfy (row + col) % 2 == 1.
        if (row + col).is_multiple_of(2) {
            return None;
        }
        Some(Self(row * 4 + (col / 2)))
    }

    /// Returns the 0-based index (`0..32`).
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Returns standard draughts notation (`1..=32`).
    #[must_use]
    pub const fn notation(self) -> u8 {
        self.0 + 1
    }

    /// Returns the single-bit mask (`1_u32 << index`) for this square.
    #[must_use]
    pub const fn bit(self) -> u32 {
        1_u32 << self.0
    }

    /// Returns 0-based `(row, col)` coordinates on the 8x8 board.
    #[must_use]
    pub const fn row_col(self) -> (u8, u8) {
        let row = self.0 / 4;
        let col_in_row = self.0 % 4;
        let col = if row.is_multiple_of(2) {
            col_in_row * 2 + 1
        } else {
            col_in_row * 2
        };
        (row, col)
    }

    /// Returns the destination square offset by `(d_row, d_col)` if in-bounds and dark.
    #[must_use]
    pub const fn offset(self, d_row: i8, d_col: i8) -> Option<Self> {
        let (row, col) = self.row_col();
        let next_row = row.cast_signed() + d_row;
        let next_col = col.cast_signed() + d_col;
        if next_row < 0 || next_row >= 8 || next_col < 0 || next_col >= 8 {
            return None;
        }
        Self::from_row_col(next_row.cast_unsigned(), next_col.cast_unsigned())
    }
}

/// Piece color in English Draughts (Black moves first from squares `1..=12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckersColor {
    /// Dark pieces starting on squares `1..=12`, advancing toward row `7`.
    Black,
    /// Light/Red pieces starting on squares `21..=32`, advancing toward row `0`.
    White,
}

impl CheckersColor {
    /// Returns the opposing color.
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }

    /// Returns the row index where men of this color are crowned king.
    #[must_use]
    pub const fn king_row(self) -> u8 {
        match self {
            Self::Black => 7,
            Self::White => 0,
        }
    }

    /// Returns the forward row delta (`+1` for Black, `-1` for White).
    #[must_use]
    pub const fn forward_delta(self) -> i8 {
        match self {
            Self::Black => 1,
            Self::White => -1,
        }
    }
}

/// Individual piece on a dark square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Piece {
    /// Uncrowned Black man (`'b'`).
    BlackMan,
    /// Crowned Black king (`'B'`).
    BlackKing,
    /// Uncrowned White/Red man (`'w'`).
    WhiteMan,
    /// Crowned White/Red king (`'W'`).
    WhiteKing,
}

impl Piece {
    /// Returns the owning color of this piece.
    #[must_use]
    pub const fn color(self) -> CheckersColor {
        match self {
            Self::BlackMan | Self::BlackKing => CheckersColor::Black,
            Self::WhiteMan | Self::WhiteKing => CheckersColor::White,
        }
    }

    /// Returns `true` if this piece is a king.
    #[must_use]
    pub const fn is_king(self) -> bool {
        matches!(self, Self::BlackKing | Self::WhiteKing)
    }

    /// Converts the piece to its protocol single-character code (`b`, `B`, `w`, `W`).
    #[must_use]
    pub const fn to_char(self) -> char {
        match self {
            Self::BlackMan => 'b',
            Self::BlackKing => 'B',
            Self::WhiteMan => 'w',
            Self::WhiteKing => 'W',
        }
    }
}

/// Deterministic ordered key identifying a board position for threefold repetition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PositionKey {
    black_men: u32,
    black_kings: u32,
    white_men: u32,
    white_kings: u32,
    turn: CheckersColor,
}

/// Bitboard state of an 8x8 English Draughts board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Board {
    /// Bitmask of uncrowned Black men.
    pub black_men: u32,
    /// Bitmask of crowned Black kings.
    pub black_kings: u32,
    /// Bitmask of uncrowned White men.
    pub white_men: u32,
    /// Bitmask of crowned White kings.
    pub white_kings: u32,
    /// Side to move.
    pub turn: CheckersColor,
}

impl Default for Board {
    fn default() -> Self {
        Self::starting()
    }
}

impl Board {
    /// Standard starting position: 12 Black men on `1..=12`, 12 White men on `21..=32`, Black to move.
    #[must_use]
    pub const fn starting() -> Self {
        Self {
            black_men: 0x0000_0FFF,
            black_kings: 0,
            white_men: 0xFFF0_0000,
            white_kings: 0,
            turn: CheckersColor::Black,
        }
    }

    /// Empty board with `turn` to move (useful for tests and custom setups).
    #[must_use]
    pub const fn empty(turn: CheckersColor) -> Self {
        Self {
            black_men: 0,
            black_kings: 0,
            white_men: 0,
            white_kings: 0,
            turn,
        }
    }

    /// Returns all Black pieces (`black_men | black_kings`).
    #[must_use]
    pub const fn black_pieces(self) -> u32 {
        self.black_men | self.black_kings
    }

    /// Returns all White pieces (`white_men | white_kings`).
    #[must_use]
    pub const fn white_pieces(self) -> u32 {
        self.white_men | self.white_kings
    }

    /// Returns all occupied squares.
    #[must_use]
    pub const fn occupied(self) -> u32 {
        self.black_pieces() | self.white_pieces()
    }

    /// Returns all pieces belonging to `color`.
    #[must_use]
    pub const fn pieces_of(self, color: CheckersColor) -> u32 {
        match color {
            CheckersColor::Black => self.black_pieces(),
            CheckersColor::White => self.white_pieces(),
        }
    }

    /// Returns men belonging to `color`.
    #[must_use]
    pub const fn men_of(self, color: CheckersColor) -> u32 {
        match color {
            CheckersColor::Black => self.black_men,
            CheckersColor::White => self.white_men,
        }
    }

    /// Returns kings belonging to `color`.
    #[must_use]
    pub const fn kings_of(self, color: CheckersColor) -> u32 {
        match color {
            CheckersColor::Black => self.black_kings,
            CheckersColor::White => self.white_kings,
        }
    }

    /// Returns the piece on `sq`, if any.
    #[must_use]
    pub const fn piece_at(self, sq: Square) -> Option<Piece> {
        let bit = sq.bit();
        if (self.black_men & bit) != 0 {
            Some(Piece::BlackMan)
        } else if (self.black_kings & bit) != 0 {
            Some(Piece::BlackKing)
        } else if (self.white_men & bit) != 0 {
            Some(Piece::WhiteMan)
        } else if (self.white_kings & bit) != 0 {
            Some(Piece::WhiteKing)
        } else {
            None
        }
    }

    /// Sets or clears a square on the board while preserving disjoint bitboards.
    pub fn set_piece(&mut self, sq: Square, piece: Option<Piece>) {
        let mask = !sq.bit();
        self.black_men &= mask;
        self.black_kings &= mask;
        self.white_men &= mask;
        self.white_kings &= mask;
        if let Some(p) = piece {
            let bit = sq.bit();
            match p {
                Piece::BlackMan => self.black_men |= bit,
                Piece::BlackKing => self.black_kings |= bit,
                Piece::WhiteMan => self.white_men |= bit,
                Piece::WhiteKing => self.white_kings |= bit,
            }
        }
    }

    /// Returns `true` if all four piece bitboards are pairwise disjoint.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        let bm = self.black_men;
        let bk = self.black_kings;
        let wm = self.white_men;
        let wk = self.white_kings;
        (bm & bk) == 0
            && (bm & wm) == 0
            && (bm & wk) == 0
            && (bk & wm) == 0
            && (bk & wk) == 0
            && (wm & wk) == 0
    }

    /// Returns the canonical [`PositionKey`] for threefold repetition tracking.
    #[must_use]
    pub const fn position_key(self) -> PositionKey {
        PositionKey {
            black_men: self.black_men,
            black_kings: self.black_kings,
            white_men: self.white_men,
            white_kings: self.white_kings,
            turn: self.turn,
        }
    }

    /// Serializes the board into the 32-character protocol string (`b`, `B`, `w`, `W`, `.`).
    #[must_use]
    pub fn to_board_string(self) -> String {
        let mut out = String::with_capacity(usize::from(Square::COUNT));
        for idx in 0..Square::COUNT {
            let ch = Square::from_index(idx)
                .and_then(|sq| self.piece_at(sq))
                .map_or('.', Piece::to_char);
            out.push(ch);
        }
        out
    }

    /// Parses a 32-character protocol board string.
    ///
    /// # Errors
    /// Returns [`CheckersError::InvalidBoard`] if the string is not 32 valid characters.
    pub fn from_board_string(input: &str, turn: CheckersColor) -> Result<Self, CheckersError> {
        let mut board = Self::empty(turn);
        let mut count = 0_u8;
        for ch in input.chars() {
            let Some(sq) = Square::from_index(count) else {
                return Err(CheckersError::InvalidBoard);
            };
            let piece = match ch {
                '.' => None,
                'b' => Some(Piece::BlackMan),
                'B' => Some(Piece::BlackKing),
                'w' => Some(Piece::WhiteMan),
                'W' => Some(Piece::WhiteKing),
                _ => return Err(CheckersError::InvalidBoard),
            };
            board.set_piece(sq, piece);
            count += 1;
        }
        if count != Square::COUNT {
            return Err(CheckersError::InvalidBoard);
        }
        Ok(board)
    }

    /// Returns `(black_total, white_total)` remaining piece counts.
    #[must_use]
    pub const fn piece_counts(self) -> (u8, u8) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "u32::count_ones() is at most 32, which always fits in u8"
        )]
        let black = self.black_pieces().count_ones() as u8;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "u32::count_ones() is at most 32, which always fits in u8"
        )]
        let white = self.white_pieces().count_ones() as u8;
        (black, white)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_notation_maps_classic_coordinates() {
        // 11 = (row 2, col 5), 15 = (row 3, col 4)
        let s11 = Square::from_notation(11).unwrap();
        let s15 = Square::from_notation(15).unwrap();
        assert_eq!(s11.row_col(), (2, 5));
        assert_eq!(s15.row_col(), (3, 4));
        assert_eq!(s11.offset(1, -1), Some(s15));

        // 23 = (row 5, col 4), 19 = (row 4, col 5)
        let s23 = Square::from_notation(23).unwrap();
        let s19 = Square::from_notation(19).unwrap();
        assert_eq!(s23.row_col(), (5, 4));
        assert_eq!(s19.row_col(), (4, 5));
        assert_eq!(s23.offset(-1, 1), Some(s19));
    }

    #[test]
    fn starting_board_round_trips_string() {
        let board = Board::starting();
        assert!(board.is_valid());
        assert_eq!(board.piece_counts(), (12, 12));
        let encoded = board.to_board_string();
        assert_eq!(encoded, "bbbbbbbbbbbb........wwwwwwwwwwww");
        let decoded = Board::from_board_string(&encoded, CheckersColor::Black).unwrap();
        assert_eq!(decoded, board);
    }
}
