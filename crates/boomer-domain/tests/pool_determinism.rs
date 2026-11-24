//! Cross-target bit-identical determinism test: 1,000 seeded pool shots (breaks,
//! heavy follow/draw, strong side English, banks, and pocket jaws).
//!
//! Runs identically on native (`cargo test -p boomer-domain --test pool_determinism`)
//! and WebAssembly (`wasm-pack test --node crates/boomer-domain --test pool_determinism`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use boomer_domain::Seat;
use boomer_domain::pool::{PoolGame, PoolSettings, ShotInput};
use boomer_domain::rng::SplitMix64;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

fn fnv_u64(hash: &mut u64, val: u64) {
    for byte in val.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn run_determinism_suite(shot_count: u32) -> u64 {
    let mut rng = SplitMix64::new(0x2026_8BA1_1000_0001);
    let mut hash = FNV_OFFSET;

    // 25 fresh racks × 40 shots = 1,000 seeded shots exercising breaks, spin, rails, and rules.
    let racks = shot_count / 40;
    for rack_idx in 0..racks {
        let mut game = PoolGame::new(
            Seat::One,
            PoolSettings::default(),
            false,
            0xAAAA_0000_u64 + u64::from(rack_idx),
        );

        for shot_idx in 0..40_u32 {
            if game.outcome().is_some() {
                game.rerack(Seat::One);
            }
            let angle = if shot_idx == 0 {
                // Aimed near the rack apex for energetic 15-ball break scatter.
                (rng.next_unit_f64() - 0.5) * 0.08
            } else {
                rng.next_unit_f64() * core::f64::consts::TAU
            };
            let power = if shot_idx == 0 {
                0.85 + rng.next_unit_f64() * 0.15
            } else {
                0.15 + rng.next_unit_f64() * 0.85
            };
            let spin_x = rng.next_unit_f64() * 2.0 - 1.0;
            let spin_y = rng.next_unit_f64() * 2.0 - 1.0;

            let shot = ShotInput {
                angle,
                power,
                spin_x,
                spin_y,
                cue_pos: None,
                called_pocket: Some((shot_idx % 6) as u8),
            };
            let res = game.shoot(shot, false).unwrap();

            fnv_u64(&mut hash, res.fouls.len() as u64);
            fnv_u64(&mut hash, res.potted.len() as u64);
            fnv_u64(&mut hash, u64::from(res.reracked));
            fnv_u64(&mut hash, u64::from(res.keeps_turn));

            for b in game.balls() {
                fnv_u64(&mut hash, u64::from(b.id));
                fnv_u64(&mut hash, u64::from(b.pocketed));
                fnv_u64(&mut hash, b.pos.x.to_bits());
                fnv_u64(&mut hash, b.pos.y.to_bits());
            }
        }
    }

    hash
}

/// Checked-in FNV-1a 64-bit golden digest across 1,000 seeded shots (breaks,
/// follow/draw, side English, rail banks, pocket jaws, and 8-ball rules).
/// Must match identically on native `x86_64` and `wasm32-unknown-unknown`.
const GOLDEN_DIGEST_1000_SHOTS: u64 = 0x3374_7dcc_07bd_cadf;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn thousand_pool_shots_are_bit_deterministic() {
    let first = run_determinism_suite(1_000);
    let second = run_determinism_suite(1_000);
    assert_eq!(first, second);
    assert_eq!(
        first, GOLDEN_DIGEST_1000_SHOTS,
        "computed digest: {first:#018x}"
    );
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn thousand_pool_shots_match_golden_digest_on_wasm() {
    let digest = run_determinism_suite(1_000);
    assert_eq!(
        digest, GOLDEN_DIGEST_1000_SHOTS,
        "computed wasm digest: {digest:#018x}"
    );
}
