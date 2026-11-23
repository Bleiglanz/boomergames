//! Explicit `From`/`TryFrom` conversions between `boomer-protocol` DTOs and `boomer-domain`.

use boomer_domain::{BotLevel, GameKind, LocalMode, Seat};
use boomer_protocol::{BotLevelDto, GameKindDto, LocalModeDto, SeatDto};

/// Converts a protocol [`GameKindDto`] into a domain [`GameKind`].
#[must_use]
pub const fn game_kind_from_dto(dto: GameKindDto) -> GameKind {
    match dto {
        GameKindDto::Checkers => GameKind::Checkers,
        GameKindDto::Pool => GameKind::Pool,
    }
}

/// Converts a domain [`GameKind`] into a protocol [`GameKindDto`].
#[must_use]
pub const fn game_kind_to_dto(kind: GameKind) -> GameKindDto {
    match kind {
        GameKind::Checkers => GameKindDto::Checkers,
        GameKind::Pool => GameKindDto::Pool,
    }
}

/// Converts a protocol [`SeatDto`] into a domain [`Seat`].
#[must_use]
pub const fn seat_from_dto(dto: SeatDto) -> Seat {
    match dto {
        SeatDto::One => Seat::One,
        SeatDto::Two => Seat::Two,
    }
}

/// Converts a domain [`Seat`] into a protocol [`SeatDto`].
#[must_use]
pub const fn seat_to_dto(seat: Seat) -> SeatDto {
    match seat {
        Seat::One => SeatDto::One,
        Seat::Two => SeatDto::Two,
    }
}

/// Converts a protocol [`BotLevelDto`] into a domain [`BotLevel`].
#[must_use]
pub const fn bot_level_from_dto(dto: BotLevelDto) -> BotLevel {
    match dto {
        BotLevelDto::Easy => BotLevel::Easy,
        BotLevelDto::Medium => BotLevel::Medium,
        BotLevelDto::Hard => BotLevel::Hard,
    }
}

/// Converts a domain [`BotLevel`] into a protocol [`BotLevelDto`].
#[must_use]
pub const fn bot_level_to_dto(level: BotLevel) -> BotLevelDto {
    match level {
        BotLevel::Easy => BotLevelDto::Easy,
        BotLevel::Medium => BotLevelDto::Medium,
        BotLevel::Hard => BotLevelDto::Hard,
    }
}

/// Converts a protocol [`LocalModeDto`] into a domain [`LocalMode`].
#[must_use]
pub const fn local_mode_from_dto(dto: LocalModeDto) -> LocalMode {
    match dto {
        LocalModeDto::Hotseat => LocalMode::Hotseat,
        LocalModeDto::VsBot { level } => LocalMode::VsBot(bot_level_from_dto(level)),
        LocalModeDto::FreePractice => LocalMode::FreePractice,
    }
}

/// Converts a domain [`LocalMode`] into a protocol [`LocalModeDto`].
#[must_use]
pub const fn local_mode_to_dto(mode: LocalMode) -> LocalModeDto {
    match mode {
        LocalMode::Hotseat => LocalModeDto::Hotseat,
        LocalMode::VsBot(level) => LocalModeDto::VsBot {
            level: bot_level_to_dto(level),
        },
        LocalMode::FreePractice => LocalModeDto::FreePractice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_mapping_preserves_values() {
        assert_eq!(
            game_kind_to_dto(game_kind_from_dto(GameKindDto::Checkers)),
            GameKindDto::Checkers
        );
        assert_eq!(
            game_kind_to_dto(game_kind_from_dto(GameKindDto::Pool)),
            GameKindDto::Pool
        );
        assert_eq!(seat_to_dto(seat_from_dto(SeatDto::One)), SeatDto::One);
        assert_eq!(seat_to_dto(seat_from_dto(SeatDto::Two)), SeatDto::Two);
        let mode = LocalModeDto::VsBot {
            level: BotLevelDto::Hard,
        };
        assert_eq!(local_mode_to_dto(local_mode_from_dto(mode)), mode);
    }
}
