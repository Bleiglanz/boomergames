//! Explicit conversions between server persistence records, `boomer-domain`, and `boomer-protocol`.

use boomer_domain::checkers::{
    CheckersColor, CheckersMove, CheckersOutcome, DrawReason, Square, WinReason,
};
use boomer_domain::math::Vec2;
use boomer_domain::pool::{
    AimAidLevel, BallGroup, BallInHandRule, BallState, EightOnBreakRule, PoolFoul, PoolOutcome,
    PoolSettings, PoolWinReason, ShotInput,
};
use boomer_domain::{BotLevel, GameKind, Seat};
use boomer_protocol::{
    AimAidLevelDto, BallGroupDto, BallInHandRuleDto, BotLevelDto, CheckersClockModeDto,
    CheckersColorDto, CheckersDrawReasonDto, CheckersMoveDto, CheckersOutcomeDto,
    CheckersSettingsDto, CheckersWinReasonDto, EightOnBreakRuleDto, GameKindDto, HighscoreRowDto,
    PoolBallDto, PoolFoulDto, PoolOutcomeDto, PoolSettingsDto, PoolShotDto, PoolWinReasonDto,
    ProfileGameStatDto, ProfileMatchDto, SeatDto, TableSettingsDto, UserDto, UserStatusDto,
};

use crate::ports::{HighscoreEntry, MatchHistoryItem, PlayerStatRecord, UserRecord};

/// Converts a wire [`GameKindDto`] to domain [`GameKind`].
#[must_use]
pub const fn game_kind_from_dto(dto: GameKindDto) -> GameKind {
    match dto {
        GameKindDto::Checkers => GameKind::Checkers,
        GameKindDto::Pool => GameKind::Pool,
    }
}

/// Converts a domain [`GameKind`] to wire [`GameKindDto`].
#[must_use]
pub const fn game_kind_to_dto(kind: GameKind) -> GameKindDto {
    match kind {
        GameKind::Checkers => GameKindDto::Checkers,
        GameKind::Pool => GameKindDto::Pool,
    }
}

/// Converts a [`GameKind`] to its PostgreSQL `text` discriminator (`"pool"` or `"checkers"`).
#[must_use]
pub const fn game_kind_to_db(kind: GameKind) -> &'static str {
    match kind {
        GameKind::Checkers => "checkers",
        GameKind::Pool => "pool",
    }
}

/// Parses a PostgreSQL `text` game discriminator into [`GameKind`].
#[must_use]
pub fn game_kind_from_db(raw: &str) -> Option<GameKind> {
    match raw {
        "checkers" => Some(GameKind::Checkers),
        "pool" => Some(GameKind::Pool),
        _ => None,
    }
}

/// Formats a registered user ID into the wire string (`"u123"`).
#[must_use]
pub fn format_user_id(id: i64) -> String {
    format!("u{id}")
}

/// Parses a registered wire user ID (`"u123"`) into `i64`.
#[must_use]
pub fn parse_user_id(wire: &str) -> Option<i64> {
    wire.strip_prefix('u')?.parse::<i64>().ok()
}

/// Formats a guest session into a [`UserDto`].
#[must_use]
pub fn guest_to_user_dto(guest_num: u32, status: UserStatusDto) -> UserDto {
    UserDto {
        id: format!("g{guest_num}"),
        name: format!("Guest_{guest_num}"),
        avatar: 0,
        rating: 1200,
        prov: true,
        status,
        bot: false,
        guest: true,
    }
}

/// Formats a registered [`UserRecord`] into a [`UserDto`].
#[must_use]
pub fn user_record_to_dto(
    user: &UserRecord,
    rating: i32,
    rated_games: i32,
    status: UserStatusDto,
) -> UserDto {
    let avatar = u8::try_from(user.avatar).unwrap_or(0);
    UserDto {
        id: format_user_id(user.id),
        name: user.display_name.clone(),
        avatar,
        rating,
        prov: rated_games < 10,
        status,
        bot: false,
        guest: false,
    }
}

/// Converts a [`PlayerStatRecord`] into a [`ProfileGameStatDto`].
#[must_use]
pub fn stat_record_to_profile_dto(stat: &PlayerStatRecord) -> ProfileGameStatDto {
    let games = u32::try_from(stat.games).unwrap_or(0);
    let wins = u32::try_from(stat.wins).unwrap_or(0);
    let losses = games.saturating_sub(wins);
    ProfileGameStatDto {
        game: game_kind_to_dto(stat.game),
        rating: stat.rating,
        games,
        wins,
        losses,
        draws: 0,
    }
}

/// Converts a [`MatchHistoryItem`] into a [`ProfileMatchDto`].
#[must_use]
pub fn match_item_to_dto(item: &MatchHistoryItem) -> ProfileMatchDto {
    ProfileMatchDto {
        game: game_kind_to_dto(item.game),
        opp: item.opponent_name.clone(),
        result: item.result.clone(),
        ts: item.ended_at.timestamp_millis(),
    }
}

/// Converts a [`HighscoreEntry`] into a [`HighscoreRowDto`].
#[must_use]
pub fn highscore_entry_to_dto(entry: &HighscoreEntry) -> HighscoreRowDto {
    HighscoreRowDto {
        rank: entry.rank,
        id: format_user_id(entry.user_id),
        name: entry.display_name.clone(),
        avatar: u8::try_from(entry.avatar).unwrap_or(0),
        wins: entry.wins,
        games: entry.games,
    }
}

/// Converts a protocol [`BotLevelDto`] into domain [`BotLevel`].
#[must_use]
pub const fn bot_level_from_dto(dto: BotLevelDto) -> BotLevel {
    match dto {
        BotLevelDto::Easy => BotLevel::Easy,
        BotLevelDto::Medium => BotLevel::Medium,
        BotLevelDto::Hard => BotLevel::Hard,
    }
}

/// Converts a protocol [`BotLevelDto`] into its PostgreSQL `smallint` (`1..=3`).
#[must_use]
pub const fn bot_level_to_db(dto: BotLevelDto) -> i16 {
    match dto {
        BotLevelDto::Easy => 1,
        BotLevelDto::Medium => 2,
        BotLevelDto::Hard => 3,
    }
}

/// Converts a domain [`Seat`] into wire [`SeatDto`].
#[must_use]
pub const fn seat_to_dto(seat: Seat) -> SeatDto {
    match seat {
        Seat::One => SeatDto::One,
        Seat::Two => SeatDto::Two,
    }
}

/// Converts a domain [`CheckersColor`] into [`CheckersColorDto`].
#[must_use]
pub const fn checkers_color_to_dto(color: CheckersColor) -> CheckersColorDto {
    match color {
        CheckersColor::Black => CheckersColorDto::Black,
        CheckersColor::White => CheckersColorDto::White,
    }
}

/// Converts a domain [`CheckersMove`] into [`CheckersMoveDto`].
#[must_use]
pub fn checkers_move_to_dto(mv: &CheckersMove) -> CheckersMoveDto {
    CheckersMoveDto {
        path: mv.path.iter().map(|sq| sq.notation()).collect(),
        captured: mv.captured.iter().map(|sq| sq.notation()).collect(),
        crowned: mv.crowned,
        notation: mv.notation(),
    }
}

/// Parses `1..=32` notation squares into domain [`Square`]s.
#[must_use]
pub fn notation_path_to_squares(path: &[u8]) -> Option<Vec<Square>> {
    path.iter().map(|&n| Square::from_notation(n)).collect()
}

/// Converts a domain [`CheckersOutcome`] into [`CheckersOutcomeDto`].
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

/// Extracts [`CheckersSettingsDto`] from [`TableSettingsDto`].
#[must_use]
pub const fn checkers_settings_from_table(settings: &TableSettingsDto) -> CheckersSettingsDto {
    let clock_mode = if settings.timer == 0 {
        CheckersClockModeDto::Unlimited
    } else if let Some(mode) = settings.opts.clock {
        mode
    } else {
        CheckersClockModeDto::Move
    };
    let game_minutes = match settings.opts.game_min {
        Some(m) if m > 0 => m,
        _ => 10,
    };
    CheckersSettingsDto {
        clock_mode,
        move_timer_secs: if settings.timer > 0 {
            settings.timer
        } else {
            30
        },
        game_minutes,
    }
}

/// Extracts domain [`PoolSettings`] from [`TableSettingsDto`].
#[must_use]
pub const fn pool_settings_from_table(settings: &TableSettingsDto) -> PoolSettings {
    let aim_aid = match settings.opts.aim {
        Some(AimAidLevelDto::Off) => AimAidLevel::Off,
        Some(AimAidLevelDto::Basic) => AimAidLevel::Basic,
        Some(AimAidLevelDto::Extended) | None => AimAidLevel::Extended,
    };
    let ball_in_hand = match settings.opts.bih {
        Some(BallInHandRuleDto::BehindHeadString) => BallInHandRule::BehindHeadString,
        Some(BallInHandRuleDto::Anywhere) | None => BallInHandRule::Anywhere,
    };
    PoolSettings {
        aim_aid,
        call_eight: match settings.opts.call_eight {
            Some(v) => v,
            None => false,
        },
        ball_in_hand,
        eight_on_break: EightOnBreakRule::ReRack,
        shot_timer_secs: settings.timer,
    }
}

/// Converts domain [`PoolSettings`] into [`PoolSettingsDto`].
#[must_use]
pub const fn pool_settings_to_dto(settings: PoolSettings) -> PoolSettingsDto {
    PoolSettingsDto {
        aim_aid: match settings.aim_aid {
            AimAidLevel::Off => AimAidLevelDto::Off,
            AimAidLevel::Basic => AimAidLevelDto::Basic,
            AimAidLevel::Extended => AimAidLevelDto::Extended,
        },
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

/// Converts a domain [`BallGroup`] into [`BallGroupDto`].
#[must_use]
pub const fn ball_group_to_dto(group: BallGroup) -> BallGroupDto {
    match group {
        BallGroup::Solids => BallGroupDto::Solids,
        BallGroup::Stripes => BallGroupDto::Stripes,
    }
}

/// Converts a domain [`PoolFoul`] into [`PoolFoulDto`].
#[must_use]
pub const fn pool_foul_to_dto(foul: PoolFoul) -> PoolFoulDto {
    match foul {
        PoolFoul::Scratch => PoolFoulDto::Scratch,
        PoolFoul::WrongFirstBall => PoolFoulDto::WrongFirstBall,
        PoolFoul::NoHit => PoolFoulDto::NoHit,
        PoolFoul::NoRailAfterContact => PoolFoulDto::NoRailAfterContact,
    }
}

/// Converts a domain [`PoolOutcome`] into [`PoolOutcomeDto`].
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

/// Converts domain [`BallState`]s into wire [`PoolBallDto`]s.
#[must_use]
pub fn pool_balls_to_dto(balls: &[BallState]) -> Vec<PoolBallDto> {
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

/// Converts a wire [`PoolShotDto`] into domain [`ShotInput`].
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

/// Converts a domain [`ShotInput`] into wire [`PoolShotDto`].
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
