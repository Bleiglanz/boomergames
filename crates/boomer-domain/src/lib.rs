//! Pure, deterministic game domain for Boomergames.
//!
//! This crate intentionally contains zero I/O, zero serialization (`serde`),
//! zero async runtime, and zero browser/WASM runtime dependencies.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod checkers;
pub mod math;
pub mod pool;
pub mod rating;
pub mod rng;

/// Supported classic games on the Boomergames platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameKind {
    /// English draughts / American checkers (8x8 board).
    Checkers,
    /// Standard WPA/BCA 8-ball pool.
    Pool,
}

/// Player seat at a two-player game table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Seat {
    /// First seat (Table host / Black in checkers by default).
    One,
    /// Second seat (White in checkers by default).
    Two,
}

impl Seat {
    /// Returns the opposing seat.
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::One,
        }
    }
}

/// Difficulty tier for built-in computer opponents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BotLevel {
    /// Approachable beginner-level opponent.
    Easy,
    /// Intermediate club-level opponent.
    Medium,
    /// Strong tactical opponent.
    Hard,
}

/// Offline single-player session modes supported without a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalMode {
    /// Two humans taking turns on the same device.
    Hotseat,
    /// Human (Seat One) versus the deterministic Rust AI (Seat Two).
    VsBot(BotLevel),
    /// Solo open-rack practice mode (Pool only).
    FreePractice,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seat_opponent_is_involution() {
        assert_eq!(Seat::One.opponent(), Seat::Two);
        assert_eq!(Seat::Two.opponent(), Seat::One);
        assert_eq!(Seat::One.opponent().opponent(), Seat::One);
    }
}
