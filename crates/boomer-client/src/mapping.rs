//! Explicit conversions between `boomer-protocol` DTOs and `boomer-domain` types.

use boomer_domain::checkers::{
    CheckersColor, CheckersMove, CheckersOutcome, DrawReason, Square, WinReason,
};
use boomer_domain::math::Vec2;
use boomer_domain::pool::constants::BALL_COUNT;
use boomer_domain::pool::{
    AimAidLevel, BallGroup, BallInHandRule, BallState, EightOnBreakRule, PoolFoul, PoolOutcome,
    PoolSettings, PoolWinReason, ShotInput,
};
use boomer_domain::{BotLevel, GameKind, LocalMode, Seat};
use boomer_protocol::{
    AimAidLevelDto, BallGroupDto, BallInHandRuleDto, BotLevelDto, CheckersColorDto,
    CheckersDrawReasonDto, CheckersMoveDto, CheckersOutcomeDto, CheckersWinReasonDto,
    EightOnBreakRuleDto, GameKindDto, LocalModeDto, PoolBallDto, PoolFoulDto, PoolOutcomeDto,
    PoolSettingsDto, PoolShotDto, PoolWinReasonDto, SeatDto,
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

/// Converts a domain [`BallGroup`] into a protocol [`BallGroupDto`].
#[must_use]
pub const fn ball_group_to_dto(group: BallGroup) -> BallGroupDto {
    match group {
        BallGroup::Solids => BallGroupDto::Solids,
        BallGroup::Stripes => BallGroupDto::Stripes,
    }
}

/// Converts a protocol [`AimAidLevelDto`] into a domain [`AimAidLevel`].
#[must_use]
pub const fn aim_aid_from_dto(dto: AimAidLevelDto) -> AimAidLevel {
    match dto {
        AimAidLevelDto::Off => AimAidLevel::Off,
        AimAidLevelDto::Basic => AimAidLevel::Basic,
        AimAidLevelDto::Extended => AimAidLevel::Extended,
    }
}

/// Converts a domain [`AimAidLevel`] into a protocol [`AimAidLevelDto`].
#[must_use]
pub const fn aim_aid_to_dto(level: AimAidLevel) -> AimAidLevelDto {
    match level {
        AimAidLevel::Off => AimAidLevelDto::Off,
        AimAidLevel::Basic => AimAidLevelDto::Basic,
        AimAidLevel::Extended => AimAidLevelDto::Extended,
    }
}

/// Converts a protocol [`PoolSettingsDto`] into domain [`PoolSettings`].
#[must_use]
pub const fn pool_settings_from_dto(dto: PoolSettingsDto) -> PoolSettings {
    PoolSettings {
        aim_aid: aim_aid_from_dto(dto.aim_aid),
        call_eight: dto.call_eight,
        ball_in_hand: match dto.ball_in_hand {
            BallInHandRuleDto::Anywhere => BallInHandRule::Anywhere,
            BallInHandRuleDto::BehindHeadString => BallInHandRule::BehindHeadString,
        },
        eight_on_break: match dto.eight_on_break {
            EightOnBreakRuleDto::ReRack => EightOnBreakRule::ReRack,
            EightOnBreakRuleDto::Win => EightOnBreakRule::Win,
        },
        shot_timer_secs: dto.shot_timer_secs,
    }
}

/// Converts domain [`PoolSettings`] into a protocol [`PoolSettingsDto`].
#[must_use]
pub const fn pool_settings_to_dto(settings: PoolSettings) -> PoolSettingsDto {
    PoolSettingsDto {
        aim_aid: aim_aid_to_dto(settings.aim_aid),
        call_eight: settings.call_eight,
        ball_in_hand: match settings.ball_in_hand {
            BallInHandRule::Anywhere => BallInHandRuleDto::Anywhere,
            BallInHandRule::BehindHeadString => BallInHandRuleDto::BehindHeadString,
        },
        eight_on_break: match settings.eight_on_break {
            EightOnBreakRule::ReRack => EightOnBreakRuleDto::ReRack,
            EightOnBreakRule::Win => EightOnBreakRuleDto::Win,
        },
        shot_timer_secs: settings.shot_timer_secs,
    }
}

/// Converts domain [`PoolFoul`] into [`PoolFoulDto`].
#[must_use]
pub const fn pool_foul_to_dto(foul: PoolFoul) -> PoolFoulDto {
    match foul {
        PoolFoul::Scratch => PoolFoulDto::Scratch,
        PoolFoul::WrongFirstBall => PoolFoulDto::WrongFirstBall,
        PoolFoul::NoHit => PoolFoulDto::NoHit,
        PoolFoul::NoRailAfterContact => PoolFoulDto::NoRailAfterContact,
    }
}

/// Converts domain [`PoolOutcome`] into [`PoolOutcomeDto`].
#[must_use]
pub const fn pool_outcome_to_dto(outcome: PoolOutcome) -> PoolOutcomeDto {
    PoolOutcomeDto {
        winner_seat: seat_to_dto(outcome.winner),
        reason: match outcome.reason {
            PoolWinReason::EightBallPocketed => PoolWinReasonDto::EightBallPocketed,
            PoolWinReason::OpponentEarlyEight => PoolWinReasonDto::OpponentEarlyEight,
            PoolWinReason::OpponentFoulOnEight => PoolWinReasonDto::OpponentFoulOnEight,
            PoolWinReason::Resign => PoolWinReasonDto::Resign,
        },
    }
}

/// Converts domain [`BallState`] slice into protocol [`PoolBallDto`] list.
#[must_use]
pub fn pool_balls_to_dto(balls: &[BallState; BALL_COUNT]) -> Vec<PoolBallDto> {
    balls
        .iter()
        .map(|b| PoolBallDto {
            id: b.id,
            x: b.pos.x,
            y: b.pos.y,
            pocketed: b.pocketed,
        })
        .collect()
}

/// Converts protocol [`PoolBallDto`] slice into a domain `[BallState; 16]` array.
#[must_use]
pub fn pool_balls_from_dto(dtos: &[PoolBallDto]) -> [BallState; BALL_COUNT] {
    let mut out = core::array::from_fn(|id| {
        #[expect(clippy::cast_possible_truncation, reason = "id in 0..16")]
        BallState::pocketed(id as u8)
    });
    for dto in dtos {
        if let Some(slot) = out.get_mut(usize::from(dto.id)) {
            *slot = if dto.pocketed {
                BallState::pocketed(dto.id)
            } else {
                BallState::stationary(dto.id, Vec2::new(dto.x, dto.y))
            };
        }
    }
    out
}

/// Converts protocol [`PoolShotDto`] into domain [`ShotInput`].
#[must_use]
pub fn pool_shot_from_dto(dto: PoolShotDto) -> ShotInput {
    ShotInput {
        angle: dto.angle,
        power: dto.power,
        spin_x: dto.spin_x,
        spin_y: dto.spin_y,
        cue_pos: dto.cue_pos.map(|[x, y]| Vec2::new(x, y)),
        called_pocket: dto.called_pocket,
    }
}

/// Converts domain [`ShotInput`] into protocol [`PoolShotDto`].
#[must_use]
pub fn pool_shot_to_dto(turn: u32, shot: ShotInput) -> PoolShotDto {
    PoolShotDto {
        turn,
        angle: shot.angle,
        power: shot.power,
        spin_x: shot.spin_x,
        spin_y: shot.spin_y,
        cue_pos: shot.cue_pos.map(|p| [p.x, p.y]),
        called_pocket: shot.called_pocket,
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

        let settings = PoolSettingsDto::default();
        assert_eq!(
            pool_settings_to_dto(pool_settings_from_dto(settings)),
            settings
        );
    }
}
