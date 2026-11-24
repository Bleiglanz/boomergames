//! Standalone JSON wire protocol DTOs for Boomergames.
//!
//! This crate has zero dependency on `boomer-domain` so wire contracts remain
//! decoupled from internal domain models.

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

pub mod auth;
pub mod checkers;
pub mod envelope;
pub mod lobby;
pub mod pool;

pub use auth::{
    HighscoreRowDto, ProfileGameStatDto, ProfileMatchDto, RoomSummaryDto, UserDto, UserStatusDto,
};
pub use checkers::{
    CheckersClockModeDto, CheckersColorDto, CheckersDrawReasonDto, CheckersMoveDto,
    CheckersOutcomeDto, CheckersSettingsDto, CheckersStateDto, CheckersWinReasonDto,
};
pub use envelope::{
    BotLevelDto, ClientMessage, ErrorCode, GameKindDto, LocalModeDto, PROTOCOL_VERSION, SeatDto,
    ServerMessage, SessionKindDto,
};
pub use lobby::{
    ChatLineDto, ChatScopeDto, GameOverDto, InviteCloseReasonDto, KickReasonDto, LobbySnapshotDto,
    RatingChangeDto, TableGameOptsDto, TablePhaseDto, TableRoleDto, TableSettingsDto,
    TableSnapshotDto, TableSummaryDto, TableUpdateDto, TableVisibilityDto, WinCountDto,
};
pub use pool::{
    AimAidLevelDto, BallGroupDto, BallInHandRuleDto, EightOnBreakRuleDto, PoolBallDto, PoolFoulDto,
    PoolOutcomeDto, PoolSettingsDto, PoolShotDto, PoolStateDto, PoolWinReasonDto,
};
