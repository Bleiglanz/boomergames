//! Pure, deterministic WPA/BCA 8-ball pool physics, rules, aiming aid, and time-sliced AI.

pub mod ai;
pub mod aim;
pub mod constants;
pub mod physics;
pub mod rules;

pub use ai::{PoolSearch, PoolSearchProgress};
pub use aim::{AimAidLevel, AimPreview, compute_aim_preview};
pub use physics::{
    BallState, PhysicsEvent, ShotInput, ShotKeyframe, ShotSimulation, simulate_shot,
};
pub use rules::{
    BallGroup, BallInHandRule, EightOnBreakRule, PoolError, PoolFoul, PoolGame, PoolOutcome,
    PoolSettings, PoolWinReason, ShotOutcome,
};
