//! Standalone wire protocol DTOs for 8-Ball Pool.

use serde::{Deserialize, Serialize};

use crate::envelope::SeatDto;

/// Wire representation of 8-ball group assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BallGroupDto {
    /// Solid balls `1..=7`.
    Solids,
    /// Striped balls `9..=15`.
    Stripes,
}

/// Wire representation of aiming-aid level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AimAidLevelDto {
    /// Cue direction stub only.
    Off,
    /// Line to first contact + ghost ball.
    Basic,
    /// Ghost ball + object-ball travel & cue-ball tangent vectors.
    Extended,
}

/// Wire representation of ball-in-hand rule after fouls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BallInHandRuleDto {
    /// Place cue ball anywhere on the table.
    Anywhere,
    /// Place cue ball behind the head string only.
    BehindHeadString,
}

/// Wire representation of 8-ball-on-break rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EightOnBreakRuleDto {
    /// Re-rack and break again.
    ReRack,
    /// Instant win for the breaker.
    Win,
}

/// Configurable 8-ball pool table settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolSettingsDto {
    /// Aiming aid level.
    pub aim_aid: AimAidLevelDto,
    /// Whether the 8-ball pocket must be called.
    pub call_eight: bool,
    /// Ball-in-hand scope after fouls.
    pub ball_in_hand: BallInHandRuleDto,
    /// 8-ball on break behavior.
    pub eight_on_break: EightOnBreakRuleDto,
    /// Shot timer in seconds (`0` = unlimited).
    pub shot_timer_secs: u32,
}

impl Default for PoolSettingsDto {
    fn default() -> Self {
        Self {
            aim_aid: AimAidLevelDto::Extended,
            call_eight: false,
            ball_in_hand: BallInHandRuleDto::Anywhere,
            eight_on_break: EightOnBreakRuleDto::ReRack,
            shot_timer_secs: 30,
        }
    }
}

/// Wire representation of pool fouls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolFoulDto {
    /// Cue ball pocketed.
    Scratch,
    /// Wrong first ball hit (including hitting 8-ball first on open table).
    WrongFirstBall,
    /// No object ball struck.
    NoHit,
    /// No rail contact after hitting an object ball (and no pot).
    NoRailAfterContact,
}

/// Wire representation of 8-ball win reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolWinReasonDto {
    /// Winner legally sank the 8-ball.
    EightBallPocketed,
    /// Opponent sank the 8-ball early.
    OpponentEarlyEight,
    /// Opponent sank the 8-ball on a foul or wrong pocket.
    OpponentFoulOnEight,
    /// Opponent resigned.
    Resign,
}

/// Terminal 8-ball outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolOutcomeDto {
    /// Winning seat.
    pub winner_seat: SeatDto,
    /// Reason for victory.
    pub reason: PoolWinReasonDto,
}

/// Wire representation of a single ball resting coordinate (`metres`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolBallDto {
    /// Ball number (`0..=15`).
    pub id: u8,
    /// X coordinate in metres (`0..2.54`).
    pub x: f64,
    /// Y coordinate in metres (`0..1.27`).
    pub y: f64,
    /// Whether the ball is pocketed.
    pub pocketed: bool,
}

/// Shot input payload sent from client to transport (and broadcast for deterministic replay).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolShotDto {
    /// Expected monotonic turn counter.
    pub turn: u32,
    /// Aim angle in radians.
    pub angle: f64,
    /// Normalized power in `[0.02, 1.0]`.
    pub power: f64,
    /// Side English in `[-1.0, 1.0]`.
    pub spin_x: f64,
    /// Top/back spin in `[-1.0, 1.0]`.
    pub spin_y: f64,
    /// Optional ball-in-hand cue position (`[x, y]` metres).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_pos: Option<[f64; 2]>,
    /// Optional called pocket (`0..6`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub called_pocket: Option<u8>,
}

/// Authoritative 8-ball table state snapshot.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Flat JSON wire snapshot DTO projected directly to JS"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolStateDto {
    /// All 16 ball positions (`0..=15`).
    pub balls: Vec<PoolBallDto>,
    /// Active shooter seat (`1` or `2`).
    pub active_seat: SeatDto,
    /// Group assignments for `[Seat 1, Seat 2]`.
    pub groups: [Option<BallGroupDto>; 2],
    /// Pocketed object balls attributed to `[Seat 1, Seat 2]`.
    pub pocketed_by_seat: [Vec<u8>; 2],
    /// Whether the next shot is the opening break.
    pub is_break_shot: bool,
    /// Whether the active player currently has ball-in-hand.
    pub ball_in_hand: bool,
    /// Whether ball-in-hand is restricted behind the head string.
    pub ball_in_hand_behind_line: bool,
    /// Whether the active player is legally shooting for the 8-ball.
    pub is_on_eight_ball: bool,
    /// Whether the table is in solo Free Practice mode.
    pub free_practice: bool,
    /// Monotonic turn counter.
    pub turn_number: u32,
    /// Active table settings.
    pub settings: PoolSettingsDto,
    /// Remaining shot-clock milliseconds.
    pub turn_ms_left: u32,
    /// Fouls from the most recent shot.
    pub last_fouls: Vec<PoolFoulDto>,
    /// Object ball IDs pocketed on the most recent shot.
    pub last_potted: Vec<u8>,
    /// Whether the most recent shot triggered an 8-on-break re-rack.
    pub reracked: bool,
    /// Whether the AI opponent is currently searching for its shot.
    pub bot_thinking: bool,
    /// Terminal match outcome, if finished.
    pub outcome: Option<PoolOutcomeDto>,
}
