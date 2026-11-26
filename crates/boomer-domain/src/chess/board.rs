//! Pure 8x8 Chess board representation, attack queries, FEN serialization, and repetition keys.

/// Chess side to move or piece allegiance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChessColor {
    /// White moves first from ranks 1–2 toward rank 8.
    White,
    /// Black moves second from ranks 7–8 toward rank 1.
    Black,
}

impl ChessColor {
    /// Returns the opposing side.
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

/// Standard Chess piece type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChessPieceKind {
    /// Pawn.
    Pawn,
    /// Knight.
    Knight,
    /// Bishop.
    Bishop,
    /// Rook.
    Rook,
    /// Queen.
    Queen,
    /// King.
    King,
}

impl ChessPieceKind {
    /// Single lowercase UCI promotion letter (`q`, `r`, `b`, `n`).
    #[must_use]
    pub const fn promotion_char(self) -> Option<char> {
        match self {
            Self::Queen => Some('q'),
            Self::Rook => Some('r'),
            Self::Bishop => Some('b'),
            Self::Knight => Some('n'),
            Self::Pawn | Self::King => None,
        }
    }

    /// Parses a UCI promotion character (`q`, `r`, `b`, `n`, case-insensitive).
    #[must_use]
    pub const fn from_promotion_char(ch: char) -> Option<Self> {
        match ch {
            'q' | 'Q' => Some(Self::Queen),
            'r' | 'R' => Some(Self::Rook),
            'b' | 'B' => Some(Self::Bishop),
            'n' | 'N' => Some(Self::Knight),
            _ => None,
        }
    }

    /// Uppercase SAN piece letter (`None` for pawns).
    #[must_use]
    pub const fn san_letter(self) -> Option<char> {
        match self {
            Self::Pawn => None,
            Self::Knight => Some('N'),
            Self::Bishop => Some('B'),
            Self::Rook => Some('R'),
            Self::Queen => Some('Q'),
            Self::King => Some('K'),
        }
    }

    /// Centipawn material value for evaluation and capture sorting.
    #[must_use]
    pub const fn material_value(self) -> i32 {
        match self {
            Self::Pawn => 100,
            Self::Knight => 320,
            Self::Bishop => 330,
            Self::Rook => 500,
            Self::Queen => 900,
            Self::King => 20_000,
        }
    }
}

/// Colored piece occupying a board square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChessPiece {
    /// Owning side.
    pub color: ChessColor,
    /// Piece type.
    pub kind: ChessPieceKind,
}

impl ChessPiece {
    /// Constructs a piece.
    #[must_use]
    pub const fn new(color: ChessColor, kind: ChessPieceKind) -> Self {
        Self { color, kind }
    }

    /// FEN / board-string character (`P N B R Q K` for White, `p n b r q k` for Black).
    #[must_use]
    pub const fn to_char(self) -> char {
        match (self.color, self.kind) {
            (ChessColor::White, ChessPieceKind::Pawn) => 'P',
            (ChessColor::White, ChessPieceKind::Knight) => 'N',
            (ChessColor::White, ChessPieceKind::Bishop) => 'B',
            (ChessColor::White, ChessPieceKind::Rook) => 'R',
            (ChessColor::White, ChessPieceKind::Queen) => 'Q',
            (ChessColor::White, ChessPieceKind::King) => 'K',
            (ChessColor::Black, ChessPieceKind::Pawn) => 'p',
            (ChessColor::Black, ChessPieceKind::Knight) => 'n',
            (ChessColor::Black, ChessPieceKind::Bishop) => 'b',
            (ChessColor::Black, ChessPieceKind::Rook) => 'r',
            (ChessColor::Black, ChessPieceKind::Queen) => 'q',
            (ChessColor::Black, ChessPieceKind::King) => 'k',
        }
    }
}

/// Square index (`0 = a1`, `7 = h1`, `56 = a8`, `63 = h8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChessSquare(u8);

impl ChessSquare {
    /// Creates a square from `0..64`.
    #[must_use]
    pub const fn from_index(idx: u8) -> Option<Self> {
        if idx < 64 { Some(Self(idx)) } else { None }
    }

    /// Creates a square from `(file 0..8, rank 0..8)` where `0=a, 0=rank 1`.
    #[must_use]
    pub const fn from_file_rank(file: u8, rank: u8) -> Option<Self> {
        if file < 8 && rank < 8 {
            Some(Self(rank * 8 + file))
        } else {
            None
        }
    }

    /// Parses algebraic coordinates like `"e2"` or `"h8"`.
    #[must_use]
    pub fn from_algebraic(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        let f = (*bytes.first()?).checked_sub(b'a')?;
        let r = (*bytes.get(1)?).checked_sub(b'1')?;
        Self::from_file_rank(f, r)
    }

    /// Raw `0..64` index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Raw `0..64` index as `u8`.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// File (`0 = a` .. `7 = h`).
    #[must_use]
    pub const fn file(self) -> u8 {
        self.0 & 7
    }

    /// Rank (`0 = rank 1` .. `7 = rank 8`).
    #[must_use]
    pub const fn rank(self) -> u8 {
        self.0 >> 3
    }

    /// File character `'a'..='h'`.
    #[must_use]
    pub const fn file_char(self) -> char {
        (b'a' + self.file()) as char
    }

    /// Rank character `'1'..='8'`.
    #[must_use]
    pub const fn rank_char(self) -> char {
        (b'1' + self.rank()) as char
    }

    /// Algebraic string (e.g., `"e4"`).
    #[must_use]
    pub fn to_algebraic(self) -> String {
        let mut s = String::with_capacity(2);
        s.push(self.file_char());
        s.push(self.rank_char());
        s
    }

    /// Offsets a square by `(df, dr)`, returning `None` if off the 8x8 board.
    #[must_use]
    pub const fn offset(self, df: i8, dr: i8) -> Option<Self> {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "file and rank are always in 0..8"
        )]
        let nf = (self.file() as i8) + df;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "file and rank are always in 0..8"
        )]
        let nr = (self.rank() as i8) + dr;
        if nf < 0 || nf >= 8 || nr < 0 || nr >= 8 {
            return None;
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "nf and nr verified non-negative above"
        )]
        Self::from_file_rank(nf as u8, nr as u8)
    }
}

/// Bitflags for castling availability (`K=1, Q=2, k=4, q=8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastlingRights(u8);

impl CastlingRights {
    /// White kingside (`O-O`).
    pub const WHITE_KING: u8 = 1;
    /// White queenside (`O-O-O`).
    pub const WHITE_QUEEN: u8 = 2;
    /// Black kingside (`O-O`).
    pub const BLACK_KING: u8 = 4;
    /// Black queenside (`O-O-O`).
    pub const BLACK_QUEEN: u8 = 8;

    /// All four castling rights active.
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::WHITE_KING | Self::WHITE_QUEEN | Self::BLACK_KING | Self::BLACK_QUEEN)
    }

    /// No castling rights.
    #[must_use]
    pub const fn none() -> Self {
        Self(0)
    }

    /// Tests whether a castling bit is set.
    #[must_use]
    pub const fn has(self, flag: u8) -> bool {
        (self.0 & flag) != 0
    }

    /// Clears the given bitmask.
    pub const fn clear(&mut self, mask: u8) {
        self.0 &= !mask;
    }

    /// Raw bitmask.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// FEN castling availability field (`"KQkq"`, `"Kq"`, or `"-"`).
    #[must_use]
    pub fn to_fen(self) -> String {
        if self.0 == 0 {
            return "-".to_owned();
        }
        let mut s = String::with_capacity(4);
        if self.has(Self::WHITE_KING) {
            s.push('K');
        }
        if self.has(Self::WHITE_QUEEN) {
            s.push('Q');
        }
        if self.has(Self::BLACK_KING) {
            s.push('k');
        }
        if self.has(Self::BLACK_QUEEN) {
            s.push('q');
        }
        s
    }
}

/// Canonical position identity for threefold repetition detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChessPositionKey {
    squares: [Option<ChessPiece>; 64],
    turn: ChessColor,
    castling: u8,
    ep_square: Option<u8>,
}

/// Full Chess position state (pieces, turn, castling, en passant, move clocks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessPosition {
    /// 64 squares indexed `0 = a1` .. `63 = h8`.
    pub squares: [Option<ChessPiece>; 64],
    /// Side whose turn it currently is.
    pub turn: ChessColor,
    /// Remaining castling rights.
    pub castling: CastlingRights,
    /// En passant target square behind a pawn that just double-pushed, if any.
    pub en_passant: Option<ChessSquare>,
    /// Half-move clock since last pawn move or capture (50-move rule at 100).
    pub halfmove_clock: u16,
    /// Full-move counter starting at 1, incremented after Black moves.
    pub fullmove_number: u32,
}

impl Default for ChessPosition {
    fn default() -> Self {
        Self::starting()
    }
}

impl ChessPosition {
    /// Standard FIDE starting Chess position.
    #[must_use]
    pub fn starting() -> Self {
        let mut squares = [None; 64];
        let back_rank = [
            ChessPieceKind::Rook,
            ChessPieceKind::Knight,
            ChessPieceKind::Bishop,
            ChessPieceKind::Queen,
            ChessPieceKind::King,
            ChessPieceKind::Bishop,
            ChessPieceKind::Knight,
            ChessPieceKind::Rook,
        ];
        for (f, &kind) in back_rank.iter().enumerate() {
            if let Some(slot) = squares.get_mut(f) {
                *slot = Some(ChessPiece::new(ChessColor::White, kind));
            }
            if let Some(slot) = squares.get_mut(8 + f) {
                *slot = Some(ChessPiece::new(ChessColor::White, ChessPieceKind::Pawn));
            }
            if let Some(slot) = squares.get_mut(48 + f) {
                *slot = Some(ChessPiece::new(ChessColor::Black, ChessPieceKind::Pawn));
            }
            if let Some(slot) = squares.get_mut(56 + f) {
                *slot = Some(ChessPiece::new(ChessColor::Black, kind));
            }
        }
        Self {
            squares,
            turn: ChessColor::White,
            castling: CastlingRights::all(),
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        }
    }

    /// Reads the piece at `sq`, if any.
    #[must_use]
    pub fn piece_at(&self, sq: ChessSquare) -> Option<ChessPiece> {
        self.squares.get(sq.index()).copied().flatten()
    }

    /// Writes `piece` to `sq`.
    pub fn set_piece(&mut self, sq: ChessSquare, piece: Option<ChessPiece>) {
        if let Some(slot) = self.squares.get_mut(sq.index()) {
            *slot = piece;
        }
    }

    /// Locates the king square for `color`.
    #[must_use]
    pub fn king_square(&self, color: ChessColor) -> Option<ChessSquare> {
        for idx in 0..64_u8 {
            if let Some(sq) = ChessSquare::from_index(idx)
                && self.piece_at(sq) == Some(ChessPiece::new(color, ChessPieceKind::King))
            {
                return Some(sq);
            }
        }
        None
    }

    /// Returns `true` if `sq` is attacked by any piece belonging to `attacker`.
    #[must_use]
    pub fn is_square_attacked(&self, sq: ChessSquare, attacker: ChessColor) -> bool {
        const KNIGHT_OFFSETS: [(i8, i8); 8] = [
            (-2, -1),
            (-2, 1),
            (-1, -2),
            (-1, 2),
            (1, -2),
            (1, 2),
            (2, -1),
            (2, 1),
        ];
        const ORTHO: [(i8, i8); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        const DIAG: [(i8, i8); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];

        // 1. Pawn attacks
        let pawn_rank_delta: i8 = match attacker {
            ChessColor::White => -1,
            ChessColor::Black => 1,
        };
        for df in [-1_i8, 1] {
            if let Some(from) = sq.offset(df, pawn_rank_delta)
                && self.piece_at(from) == Some(ChessPiece::new(attacker, ChessPieceKind::Pawn))
            {
                return true;
            }
        }

        // 2. Knight attacks
        for (df, dr) in KNIGHT_OFFSETS {
            if let Some(from) = sq.offset(df, dr)
                && self.piece_at(from) == Some(ChessPiece::new(attacker, ChessPieceKind::Knight))
            {
                return true;
            }
        }

        // 3. King attacks
        for df in -1_i8..=1 {
            for dr in -1_i8..=1 {
                if df == 0 && dr == 0 {
                    continue;
                }
                if let Some(from) = sq.offset(df, dr)
                    && self.piece_at(from) == Some(ChessPiece::new(attacker, ChessPieceKind::King))
                {
                    return true;
                }
            }
        }

        // 4. Sliding attacks (rook/queen orthogonal, bishop/queen diagonal)
        for (df, dr) in ORTHO {
            let mut cur = sq;
            while let Some(next) = cur.offset(df, dr) {
                cur = next;
                if let Some(p) = self.piece_at(cur) {
                    if p.color == attacker
                        && matches!(p.kind, ChessPieceKind::Rook | ChessPieceKind::Queen)
                    {
                        return true;
                    }
                    break;
                }
            }
        }

        for (df, dr) in DIAG {
            let mut cur = sq;
            while let Some(next) = cur.offset(df, dr) {
                cur = next;
                if let Some(p) = self.piece_at(cur) {
                    if p.color == attacker
                        && matches!(p.kind, ChessPieceKind::Bishop | ChessPieceKind::Queen)
                    {
                        return true;
                    }
                    break;
                }
            }
        }

        false
    }

    /// Returns `true` if `color`'s king is currently in check.
    #[must_use]
    pub fn is_in_check(&self, color: ChessColor) -> bool {
        self.king_square(color)
            .is_some_and(|ksq| self.is_square_attacked(ksq, color.opponent()))
    }

    /// Returns a canonical repetition key.
    #[must_use]
    pub const fn position_key(&self) -> ChessPositionKey {
        ChessPositionKey {
            squares: self.squares,
            turn: self.turn,
            castling: self.castling.bits(),
            ep_square: match self.en_passant {
                Some(sq) => Some(sq.raw()),
                None => None,
            },
        }
    }

    /// 64-character board string ordered from `a8..h8`, `a7..h7`, ..., `a1..h1`.
    #[must_use]
    pub fn to_board_string(&self) -> String {
        let mut out = String::with_capacity(64);
        for rank in (0..8_u8).rev() {
            for file in 0..8_u8 {
                let ch = ChessSquare::from_file_rank(file, rank)
                    .and_then(|sq| self.piece_at(sq))
                    .map_or('.', ChessPiece::to_char);
                out.push(ch);
            }
        }
        out
    }

    /// Full Forsyth–Edwards Notation (FEN) string suitable for UCI engines like Stockfish.
    #[must_use]
    pub fn to_fen(&self) -> String {
        let mut fen = String::with_capacity(80);
        for rank in (0..8_u8).rev() {
            let mut empty = 0_u8;
            for file in 0..8_u8 {
                if let Some(piece) =
                    ChessSquare::from_file_rank(file, rank).and_then(|sq| self.piece_at(sq))
                {
                    if empty > 0 {
                        fen.push((b'0' + empty) as char);
                        empty = 0;
                    }
                    fen.push(piece.to_char());
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                fen.push((b'0' + empty) as char);
            }
            if rank > 0 {
                fen.push('/');
            }
        }

        fen.push(' ');
        fen.push(match self.turn {
            ChessColor::White => 'w',
            ChessColor::Black => 'b',
        });
        fen.push(' ');
        fen.push_str(&self.castling.to_fen());
        fen.push(' ');
        if let Some(ep) = self.en_passant {
            fen.push_str(&ep.to_algebraic());
        } else {
            fen.push('-');
        }
        fen.push(' ');
        fen.push_str(&self.halfmove_clock.to_string());
        fen.push(' ');
        fen.push_str(&self.fullmove_number.to_string());
        fen
    }

    /// Returns `true` when neither side can possibly force checkmate with remaining material.
    #[must_use]
    pub fn is_insufficient_material(&self) -> bool {
        let mut white_minors: Vec<(ChessPieceKind, bool)> = Vec::new();
        let mut black_minors: Vec<(ChessPieceKind, bool)> = Vec::new();
        for idx in 0..64_u8 {
            let Some(sq) = ChessSquare::from_index(idx) else {
                continue;
            };
            let Some(piece) = self.piece_at(sq) else {
                continue;
            };
            match piece.kind {
                ChessPieceKind::King => {}
                ChessPieceKind::Pawn | ChessPieceKind::Rook | ChessPieceKind::Queen => {
                    return false;
                }
                ChessPieceKind::Bishop | ChessPieceKind::Knight => {
                    let is_light = (sq.file() + sq.rank()) % 2 == 1;
                    match piece.color {
                        ChessColor::White => white_minors.push((piece.kind, is_light)),
                        ChessColor::Black => black_minors.push((piece.kind, is_light)),
                    }
                }
            }
        }
        let total = white_minors.len() + black_minors.len();
        if total <= 1 {
            return true;
        }
        // K+B vs K+B on same-colored squares
        if white_minors.len() == 1
            && black_minors.len() == 1
            && let (Some(&(ChessPieceKind::Bishop, wl)), Some(&(ChessPieceKind::Bishop, bl))) =
                (white_minors.first(), black_minors.first())
        {
            return wl == bl;
        }
        false
    }
}
