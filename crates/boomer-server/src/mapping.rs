//! Explicit conversions between server persistence records, `boomer-domain`, and `boomer-protocol`.

use boomer_domain::GameKind;
use boomer_protocol::{
    GameKindDto, HighscoreRowDto, ProfileGameStatDto, ProfileMatchDto, UserDto, UserStatusDto,
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
