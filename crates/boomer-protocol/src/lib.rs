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

pub mod envelope;

pub use envelope::{
    BotLevelDto, ClientMessage, ErrorCode, GameKindDto, LocalModeDto, PROTOCOL_VERSION, SeatDto,
    ServerMessage, SessionKindDto,
};
