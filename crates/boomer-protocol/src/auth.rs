//! Standalone wire protocol DTOs for accounts, profiles, high scores, and lounge summaries.

use serde::{Deserialize, Serialize};

use crate::envelope::GameKindDto;

/// Presence status of a user in a lounge roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatusDto {
    /// Available in the lounge to play or receive invites.
    Available,
    /// Seated at a table waiting for an opponent or ready click.
    Waiting,
    /// Actively playing a match.
    Playing,
    /// Spectating a table.
    Watching,
}

/// Public user representation used in `welcome`, rosters, seats, and invites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    /// Stable wire ID (`"u123"`, `"g4821"`, or `"bot2"`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Index into built-in avatar set.
    pub avatar: u8,
    /// Current Elo rating for the active game context.
    pub rating: i32,
    /// `true` if provisional (`< 10` rated games).
    pub prov: bool,
    /// Current presence status.
    pub status: UserStatusDto,
    /// `true` for clearly badged computer house bots.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bot: bool,
    /// `true` for unauthenticated guest sessions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub guest: bool,
}

/// Summary of an available lounge room returned on `welcome`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSummaryDto {
    /// Room identifier (e.g., `"pool-social-1"`).
    pub id: String,
    /// Game hosted in this lounge.
    pub game: GameKindDto,
    /// Human-readable room title.
    pub name: String,
    /// Skill tier label (`"beginner"`, `"intermediate"`, `"advanced"`, `"social"`).
    pub tier: String,
    /// Number of currently connected users.
    pub users: u32,
}

/// Per-game rating & W/L/D statistics shown on a player's profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileGameStatDto {
    /// Game kind.
    pub game: GameKindDto,
    /// Current Elo rating.
    pub rating: i32,
    /// Total games against humans.
    pub games: u32,
    /// Wins against humans.
    pub wins: u32,
    /// Losses against humans.
    pub losses: u32,
    /// Draws against humans.
    pub draws: u32,
}

/// Compact match history item shown on a player's profile (last 20 matches).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMatchDto {
    /// Game kind.
    pub game: GameKindDto,
    /// Opponent display name or bot label.
    pub opp: String,
    /// Outcome from this player's perspective (`"win"`, `"loss"`, `"draw"`).
    pub result: String,
    /// Match completion UTC Unix timestamp in milliseconds.
    pub ts: i64,
}

/// Single row in the per-game human-win High Score table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighscoreRowDto {
    /// 1-based rank.
    pub rank: u32,
    /// User wire ID (`"u123"`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Built-in avatar index.
    pub avatar: u8,
    /// Counted human wins.
    pub wins: u32,
    /// Total human games played.
    pub games: u32,
}
