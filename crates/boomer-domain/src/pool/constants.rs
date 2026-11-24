//! Regulation 9-foot pool table geometry and physical constants (SI metres & seconds).

use crate::math::Vec2;

/// Playing surface width in metres (9-foot regulation table: 100 inches).
pub const TABLE_WIDTH: f64 = 2.54;

/// Playing surface height in metres (9-foot regulation table: 50 inches).
pub const TABLE_HEIGHT: f64 = 1.27;

/// Standard pool ball radius in metres (2.25-inch diameter).
pub const BALL_RADIUS: f64 = 0.028_575;

/// Standard pool ball diameter in metres.
pub const BALL_DIAMETER: f64 = BALL_RADIUS * 2.0;

/// X-coordinate of the head string (kitchen line).
pub const HEAD_STRING_X: f64 = 0.635;

/// X-coordinate of the foot spot (rack apex).
pub const FOOT_SPOT_X: f64 = 1.905;

/// Vertical centerline of the table.
pub const CENTER_Y: f64 = 0.635;

/// Total number of balls (cue ball `0` + object balls `1..=15`).
pub const BALL_COUNT: usize = 16;

/// Corner pocket mouth inset from the table corner along each rail.
pub const CORNER_MOUTH: f64 = 0.088;

/// Half-width of the side pocket mouth opening along the long rails.
pub const SIDE_HALF_MOUTH: f64 = 0.070;

/// Radius of the rounded pocket-jaw knuckle circles at every rail endpoint.
pub const JAW_RADIUS: f64 = 0.018;

/// Fixed physics substep duration (`1 / 240 s`).
pub const PHYSICS_DT: f64 = 1.0 / 240.0;

/// Number of physics substeps per 60 Hz animation keyframe (`240 / 60 = 4`).
pub const SUBSTEPS_PER_FRAME: usize = 4;

/// Maximum simulated shot duration (`18 s` = `4320` substeps) guaranteeing termination.
pub const MAX_SUBSTEPS: usize = 4_320;

/// Maximum Swept-CCD collision iterations inside a single substep.
pub const MAX_CCD_EVENTS_PER_STEP: usize = 16;

/// Maximum cue-ball launch speed at `power = 1.0` (metres per second).
pub const MAX_SHOT_SPEED: f64 = 6.8;

/// Minimum cue-ball launch speed at `power = 0.02`.
pub const MIN_SHOT_SPEED: f64 = 0.35;

/// Gravitational acceleration (`m/s^2`).
pub const GRAVITY: f64 = 9.81;

/// Coefficient of sliding friction between ball and tournament worsted cloth.
pub const MU_SLIDE: f64 = 0.16;

/// Coefficient of rolling resistance between ball and cloth.
pub const MU_ROLL: f64 = 0.0165;

/// Decay rate of vertical-axis side English (`rad/s^2`-equivalent).
pub const SIDE_SPIN_DECAY: f64 = 1.8;

/// Ball-ball normal restitution coefficient.
pub const BALL_RESTITUTION: f64 = 0.94;

/// Cushion normal restitution coefficient.
pub const CUSHION_RESTITUTION: f64 = 0.80;

/// Pocket jaw restitution coefficient.
pub const JAW_RESTITUTION: f64 = 0.72;

/// Speed threshold below which a ball snaps cleanly to rest (`m/s`).
pub const REST_VEL_EPS: f64 = 0.008;

/// Spin-velocity threshold below which sliding spin snaps cleanly to rest (`m/s`).
pub const REST_SPIN_EPS: f64 = 0.015;

/// Definition of one of the 6 pockets on the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PocketDef {
    /// Pocket index (`0..6`).
    pub id: u8,
    /// Pocket drop center in table metres.
    pub center: Vec2,
    /// Capture radius: ball drops when `dist(ball, center) <= radius`.
    pub radius: f64,
}

/// The 6 standard pockets (0: bottom-left, 1: bottom-mid, 2: bottom-right,
/// 3: top-left, 4: top-mid, 5: top-right).
pub const POCKETS: [PocketDef; 6] = [
    PocketDef {
        id: 0,
        center: Vec2 { x: 0.0, y: 0.0 },
        radius: 0.064,
    },
    PocketDef {
        id: 1,
        center: Vec2 {
            x: TABLE_WIDTH * 0.5,
            y: -0.018,
        },
        radius: 0.068,
    },
    PocketDef {
        id: 2,
        center: Vec2 {
            x: TABLE_WIDTH,
            y: 0.0,
        },
        radius: 0.064,
    },
    PocketDef {
        id: 3,
        center: Vec2 {
            x: 0.0,
            y: TABLE_HEIGHT,
        },
        radius: 0.064,
    },
    PocketDef {
        id: 4,
        center: Vec2 {
            x: TABLE_WIDTH * 0.5,
            y: TABLE_HEIGHT + 0.018,
        },
        radius: 0.068,
    },
    PocketDef {
        id: 5,
        center: Vec2 {
            x: TABLE_WIDTH,
            y: TABLE_HEIGHT,
        },
        radius: 0.064,
    },
];

/// Aim target points slightly inside each pocket mouth (used by AI shot search).
pub const POCKET_AIM_TARGETS: [Vec2; 6] = [
    Vec2 { x: 0.032, y: 0.032 },
    Vec2 {
        x: TABLE_WIDTH * 0.5,
        y: 0.014,
    },
    Vec2 {
        x: TABLE_WIDTH - 0.032,
        y: 0.032,
    },
    Vec2 {
        x: 0.032,
        y: TABLE_HEIGHT - 0.032,
    },
    Vec2 {
        x: TABLE_WIDTH * 0.5,
        y: TABLE_HEIGHT - 0.014,
    },
    Vec2 {
        x: TABLE_WIDTH - 0.032,
        y: TABLE_HEIGHT - 0.032,
    },
];

/// The 12 rounded pocket-jaw knuckles framing the 6 pocket mouths.
pub const POCKET_JAWS: [Vec2; 12] = [
    // Bottom rail jaws
    Vec2 {
        x: CORNER_MOUTH,
        y: -JAW_RADIUS,
    },
    Vec2 {
        x: TABLE_WIDTH * 0.5 - SIDE_HALF_MOUTH,
        y: -JAW_RADIUS,
    },
    Vec2 {
        x: TABLE_WIDTH * 0.5 + SIDE_HALF_MOUTH,
        y: -JAW_RADIUS,
    },
    Vec2 {
        x: TABLE_WIDTH - CORNER_MOUTH,
        y: -JAW_RADIUS,
    },
    // Top rail jaws
    Vec2 {
        x: CORNER_MOUTH,
        y: TABLE_HEIGHT + JAW_RADIUS,
    },
    Vec2 {
        x: TABLE_WIDTH * 0.5 - SIDE_HALF_MOUTH,
        y: TABLE_HEIGHT + JAW_RADIUS,
    },
    Vec2 {
        x: TABLE_WIDTH * 0.5 + SIDE_HALF_MOUTH,
        y: TABLE_HEIGHT + JAW_RADIUS,
    },
    Vec2 {
        x: TABLE_WIDTH - CORNER_MOUTH,
        y: TABLE_HEIGHT + JAW_RADIUS,
    },
    // Left rail jaws
    Vec2 {
        x: -JAW_RADIUS,
        y: CORNER_MOUTH,
    },
    Vec2 {
        x: -JAW_RADIUS,
        y: TABLE_HEIGHT - CORNER_MOUTH,
    },
    // Right rail jaws
    Vec2 {
        x: TABLE_WIDTH + JAW_RADIUS,
        y: CORNER_MOUTH,
    },
    Vec2 {
        x: TABLE_WIDTH + JAW_RADIUS,
        y: TABLE_HEIGHT - CORNER_MOUTH,
    },
];
