//! Boomergames browser/WASM client crate.
//!
//! All client state, input validation, DTO<->domain mapping, and animation
//! interpolation live here. The TypeScript shell in `web/` only forwards raw
//! user inputs and renders the projected `ViewModel` and `Float64Array` frames.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod animation;
pub mod app;
pub mod bridge_adapter;
pub mod local_adapter;
pub mod mapping;
pub mod ports;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
#[cfg(target_arch = "wasm32")]
pub mod ws_adapter;
