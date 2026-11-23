//! Explicit conversions between `boomer-protocol` DTOs and `boomer-domain` types.

use boomer_domain::checkers::{
    CheckersColor, CheckersMove, CheckersOutcome, DrawReason, Square, WinReason,
};
use boomer_domain::{BotLevel, GameKind, LocalMode, Seat};
use boomer_protocol::{
    BotLevelDto, CheckersColorDto, CheckersDrawReasonDto, CheckersMoveDto, CheckersOutcomeDto,
    CheckersWinReasonDto, GameKindDto, LocalModeDto, SeatDto,
};

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

/// Converts a domain [`CheckersColor`] into a protocol [`CheckersColorDto`].
#[must_use]
pub const fn checkers_color_to_dto(color: CheckersColor) -> CheckersColorDto {
    match color {
        CheckersColor::Black => CheckersColorDto::Black,
        CheckersColor::White => CheckersColorDto::White,
    }
}

/// Converts a domain [`CheckersMove`] into a protocol [`CheckersMoveDto`].
#[must_use]
pub fn checkers_move_to_dto(mv: &CheckersMove) -> CheckersMoveDto {
    CheckersMoveDto {
        path: mv.path.iter().map(|sq| sq.notation()).collect(),
        captured: mv.captured.iter().map(|sq| sq.notation()).collect(),
        crowned: mv.crowned,
        notation: mv.notation(),
    }
}

/// Converts a sequence of `1..=32` notation squares into domain [`Square`]s.
#[must_use]
pub fn notation_path_to_squares(path: &[u8]) -> Option<Vec<Square>> {
    path.iter().map(|&n| Square::from_notation(n)).collect()
}

/// Converts a domain [`CheckersOutcome`] into a protocol [`CheckersOutcomeDto`].
#[must_use]
pub const fn checkers_outcome_to_dto(
    outcome: CheckersOutcome,
    seat_colors: [CheckersColor; 2],
) -> CheckersOutcomeDto {
    match outcome {
        CheckersOutcome::Win { winner, reason } => {
            let [seat_one_color, _] = seat_colors;
            let winner_seat = if seat_one_color as u8 == winner as u8 {
                SeatDto::One
            } else {
                SeatDto::Two
            };
            let dto_reason = match reason {
                WinReason::NoMoves => CheckersWinReasonDto::NoMoves,
                WinReason::Resign => CheckersWinReasonDto::Resign,
                WinReason::Timeout => CheckersWinReasonDto::Timeout,
            };
            CheckersOutcomeDto::Win {
                winner: checkers_color_to_dto(winner),
                winner_seat,
                reason: dto_reason,
            }
        }
        CheckersOutcome::Draw { reason } => {
            let dto_reason = match reason {
                DrawReason::Agreement => CheckersDrawReasonDto::Agreement,
                DrawReason::ThreefoldRepetition => CheckersDrawReasonDto::ThreefoldRepetition,
                DrawReason::FortyMoveRule => CheckersDrawReasonDto::FortyMoveRule,
            };
            CheckersOutcomeDto::Draw { reason: dto_reason }
        }
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
        assert!(notation_path_to_squares(&[11, 15]).is_some());
        assert!(notation_path_to_squares(&[0, 15]).is_none());
    }
}
