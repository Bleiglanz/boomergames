//! Deterministic 2D rigid-sphere pool physics with analytic Swept CCD,
//! sliding/rolling friction, top/back spin (follow/draw), side English, and pocket jaws.

use crate::math::{Vec2, canonical_zero, clamp, cos, fabs, sin, sqrt};
use crate::pool::constants::{
    BALL_COUNT, BALL_DIAMETER, BALL_RADIUS, BALL_RESTITUTION, CORNER_MOUTH, CUSHION_RESTITUTION,
    GRAVITY, JAW_RADIUS, JAW_RESTITUTION, MAX_CCD_EVENTS_PER_STEP, MAX_SHOT_SPEED, MAX_SUBSTEPS,
    MIN_SHOT_SPEED, MU_ROLL, MU_SLIDE, PHYSICS_DT, POCKET_JAWS, POCKETS, REST_SPIN_EPS,
    REST_VEL_EPS, SIDE_HALF_MOUTH, SIDE_SPIN_DECAY, SUBSTEPS_PER_FRAME, TABLE_HEIGHT, TABLE_WIDTH,
};

/// State of a single billiard ball during simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallState {
    /// Ball number (`0` = cue ball, `1..=15` = object balls).
    pub id: u8,
    /// Center position in table metres (`[0, 2.54] x [0, 1.27]`).
    pub pos: Vec2,
    /// Linear center-of-mass velocity in `m/s`.
    pub vel: Vec2,
    /// Linear-equivalent rolling/sliding contact spin (`m/s`). Pure roll satisfies `spin_xy == vel`.
    pub spin_xy: Vec2,
    /// Vertical-axis side English (`spin_x`), in linear-equivalent `m/s`.
    pub spin_z: f64,
    /// `true` once the ball has dropped into a pocket.
    pub pocketed: bool,
}

impl BallState {
    /// Creates a stationary ball at `pos`.
    #[must_use]
    pub fn stationary(id: u8, pos: Vec2) -> Self {
        Self {
            id,
            pos: Vec2::new(pos.x, pos.y),
            vel: Vec2::ZERO,
            spin_xy: Vec2::ZERO,
            spin_z: 0.0,
            pocketed: false,
        }
    }

    /// Creates a pocketed/inactive ball.
    #[must_use]
    pub const fn pocketed(id: u8) -> Self {
        Self {
            id,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            spin_xy: Vec2::ZERO,
            spin_z: 0.0,
            pocketed: true,
        }
    }

    fn is_moving(self) -> bool {
        !self.pocketed
            && (self.vel.len_sq() > REST_VEL_EPS * REST_VEL_EPS
                || self.spin_xy.len_sq() > REST_SPIN_EPS * REST_SPIN_EPS
                || fabs(self.spin_z) > REST_SPIN_EPS)
    }

    fn canonicalize(&mut self) {
        self.pos = Vec2::new(self.pos.x, self.pos.y);
        self.vel = Vec2::new(self.vel.x, self.vel.y);
        self.spin_xy = Vec2::new(self.spin_xy.x, self.spin_xy.y);
        self.spin_z = canonical_zero(self.spin_z);
    }
}

/// Player shot parameters submitted to the physics engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotInput {
    /// Aim angle in radians (`0` = +X right toward foot spot, `pi/2` = +Y top rail).
    pub angle: f64,
    /// Normalized shot power in `[0.02, 1.0]`.
    pub power: f64,
    /// Side English in `[-1.0, 1.0]` (`-1` = left spin, `+1` = right spin).
    pub spin_x: f64,
    /// Top/Back spin in `[-1.0, 1.0]` (`+1` = strong follow, `-1` = strong draw).
    pub spin_y: f64,
    /// Optional cue-ball placement when shooting with ball-in-hand.
    pub cue_pos: Option<Vec2>,
    /// Optional called pocket index (`0..6`) for 8-ball call-pocket rules.
    pub called_pocket: Option<u8>,
}

/// Discrete physical event recorded during a shot simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhysicsEvent {
    /// Cue ball hit an object ball for the first time on this shot.
    FirstContact {
        /// Object ball ID (`1..=15`).
        ball: u8,
        /// Time of contact in milliseconds.
        time_ms: u32,
    },
    /// Two balls collided (useful for sound cues).
    BallHit {
        /// First ball ID.
        ball_a: u8,
        /// Second ball ID.
        ball_b: u8,
        /// Normal impact speed (`m/s`).
        speed: f64,
        /// Time of impact in milliseconds.
        time_ms: u32,
    },
    /// A ball rebounded off a cushion rail or pocket jaw.
    CushionHit {
        /// Ball ID.
        ball: u8,
        /// Normal impact speed (`m/s`).
        speed: f64,
        /// Time of impact in milliseconds.
        time_ms: u32,
    },
    /// An object ball dropped into a pocket.
    Pot {
        /// Object ball ID (`1..=15`).
        ball: u8,
        /// Pocket index (`0..6`).
        pocket: u8,
        /// Time of drop in milliseconds.
        time_ms: u32,
    },
    /// The cue ball dropped into a pocket.
    Scratch {
        /// Pocket index (`0..6`).
        pocket: u8,
        /// Time of drop in milliseconds.
        time_ms: u32,
    },
}

/// One 60 Hz animation snapshot of all 16 balls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotKeyframe {
    /// Elapsed milliseconds since shot strike.
    pub time_ms: f64,
    /// `(x, y, visible)` for each ball ID `0..16`.
    pub balls: [(f64, f64, bool); BALL_COUNT],
}

/// Complete deterministic output of simulating one pool shot to rest.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotSimulation {
    /// Final resting states of all 16 balls.
    pub final_balls: [BallState; BALL_COUNT],
    /// Chronological list of collisions, cushion hits, pots, and scratches.
    pub events: Vec<PhysicsEvent>,
    /// 60 Hz sampled keyframes for smooth client animation (`empty` when `record_keyframes == false`).
    pub keyframes: Vec<ShotKeyframe>,
    /// Total simulated duration in milliseconds.
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Copy)]
enum CollisionKind {
    BallBall { i: usize, j: usize },
    Rail { i: usize, rail: RailSide },
    Jaw { i: usize, jaw_idx: usize },
}

#[derive(Debug, Clone, Copy)]
enum RailSide {
    Left,
    Right,
    Bottom,
    Top,
}

/// Runs a full deterministic simulation of `shot` starting from `initial_balls`,
/// optionally recording 60 Hz keyframes for client animation playback.
#[must_use]
pub fn simulate_shot(
    initial_balls: &[BallState; BALL_COUNT],
    shot: ShotInput,
    record_keyframes: bool,
) -> ShotSimulation {
    let mut balls = *initial_balls;
    for b in &mut balls {
        b.vel = Vec2::ZERO;
        b.spin_xy = Vec2::ZERO;
        b.spin_z = 0.0;
        b.canonicalize();
    }

    // Apply optional ball-in-hand cue position and cue-ball strike impulse.
    if let Some(cue) = balls.get_mut(0) {
        if let Some(pos) = shot.cue_pos {
            cue.pos = Vec2::new(pos.x, pos.y);
            cue.pocketed = false;
        }
        let clamped_power = clamp(shot.power, 0.02, 1.0);
        let speed =
            MIN_SHOT_SPEED + (MAX_SHOT_SPEED - MIN_SHOT_SPEED) * (clamped_power * clamped_power);
        let dir = Vec2::new(cos(shot.angle), sin(shot.angle));
        let spin_y = clamp(shot.spin_y, -1.0, 1.0);
        let spin_x = clamp(shot.spin_x, -1.0, 1.0);

        cue.vel = dir.scale(speed);
        // Follow (+spin_y) imparts forward surface speed; Draw (-spin_y) imparts backward surface speed.
        cue.spin_xy = dir.scale(speed * spin_y * 2.6);
        cue.spin_z = canonical_zero(speed * spin_x * 1.4);
    }

    let mut events = Vec::new();
    let mut keyframes = Vec::new();
    let mut first_contact_recorded = false;

    if record_keyframes {
        keyframes.push(capture_keyframe(&balls, 0.0));
    }

    let mut elapsed_s = 0.0_f64;

    for step in 0..MAX_SUBSTEPS {
        let mut remaining = PHYSICS_DT;
        let step_start_s = elapsed_s;

        for _ in 0..MAX_CCD_EVENTS_PER_STEP {
            // 1. Check pocket captures.
            check_pockets(
                &mut balls,
                step_start_s + (PHYSICS_DT - remaining),
                &mut events,
            );

            // 2. Find earliest analytic time of impact in [0, remaining].
            let Some((t_hit, kind)) = find_earliest_collision(&balls, remaining) else {
                advance_positions(&mut balls, remaining);
                break;
            };

            let advance = clamp(t_hit, 0.0, remaining);
            advance_positions(&mut balls, advance);
            remaining = canonical_zero(remaining - advance);
            let event_time_ms = seconds_to_ms(step_start_s + (PHYSICS_DT - remaining));

            // 3. Resolve collision impulse.
            match kind {
                CollisionKind::BallBall { i, j } => {
                    resolve_ball_ball(
                        &mut balls,
                        i,
                        j,
                        event_time_ms,
                        &mut first_contact_recorded,
                        &mut events,
                    );
                }
                CollisionKind::Rail { i, rail } => {
                    resolve_rail(&mut balls, i, rail, event_time_ms, &mut events);
                }
                CollisionKind::Jaw { i, jaw_idx } => {
                    resolve_jaw(&mut balls, i, jaw_idx, event_time_ms, &mut events);
                }
            }

            if remaining <= 1e-9 {
                break;
            }
        }

        // Catch any ball that slipped into a pocket at the end of the substep.
        elapsed_s = canonical_zero(elapsed_s + PHYSICS_DT);
        check_pockets(&mut balls, elapsed_s, &mut events);

        // Step sliding, rolling, and side-spin friction.
        step_friction(&mut balls, PHYSICS_DT);

        let any_moving = balls.iter().any(|b| b.is_moving());
        let is_frame_tick = (step + 1).is_multiple_of(SUBSTEPS_PER_FRAME);

        if record_keyframes && (is_frame_tick || !any_moving) {
            keyframes.push(capture_keyframe(&balls, elapsed_s * 1_000.0));
        }

        if !any_moving {
            break;
        }
    }

    // Snap all resting balls cleanly to zero velocity and canonicalize coordinates.
    for b in &mut balls {
        b.vel = Vec2::ZERO;
        b.spin_xy = Vec2::ZERO;
        b.spin_z = 0.0;
        b.canonicalize();
    }

    let duration_ms = canonical_zero(elapsed_s * 1_000.0);
    if record_keyframes {
        // Ensure final resting keyframe is exact.
        keyframes.push(capture_keyframe(&balls, duration_ms));
    }

    ShotSimulation {
        final_balls: balls,
        events,
        keyframes,
        duration_ms,
    }
}

fn seconds_to_ms(seconds: f64) -> u32 {
    let clamped = clamp(seconds * 1_000.0, 0.0, f64::from(u32::MAX));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to [0.0, u32::MAX]"
    )]
    let ms = clamped as u32;
    ms
}

fn capture_keyframe(balls: &[BallState; BALL_COUNT], time_ms: f64) -> ShotKeyframe {
    let mut coords = [(0.0, 0.0, false); BALL_COUNT];
    for (dst, src) in coords.iter_mut().zip(balls.iter()) {
        *dst = (
            canonical_zero(src.pos.x),
            canonical_zero(src.pos.y),
            !src.pocketed,
        );
    }
    ShotKeyframe {
        time_ms: canonical_zero(time_ms),
        balls: coords,
    }
}

fn advance_positions(balls: &mut [BallState; BALL_COUNT], dt: f64) {
    if dt <= 0.0 {
        return;
    }
    for b in balls {
        if !b.pocketed {
            b.pos = b.pos + b.vel.scale(dt);
        }
    }
}

fn check_pockets(
    balls: &mut [BallState; BALL_COUNT],
    elapsed_sec: f64,
    events: &mut Vec<PhysicsEvent>,
) {
    let time_ms = seconds_to_ms(elapsed_sec);
    for b in balls {
        if b.pocketed {
            continue;
        }
        let mut dropped_pocket = None;
        for pocket in &POCKETS {
            let d_sq = (b.pos - pocket.center).len_sq();
            if d_sq <= pocket.radius * pocket.radius {
                dropped_pocket = Some(pocket.id);
                break;
            }
        }

        // Guard against extreme multi-ball squeezes outside table envelope: drop to nearest pocket.
        if dropped_pocket.is_none()
            && (b.pos.x < -BALL_RADIUS
                || b.pos.x > TABLE_WIDTH + BALL_RADIUS
                || b.pos.y < -BALL_RADIUS
                || b.pos.y > TABLE_HEIGHT + BALL_RADIUS)
        {
            let mut nearest_id = 0_u8;
            let mut min_dist_sq = f64::MAX;
            for pocket in &POCKETS {
                let d_sq = (b.pos - pocket.center).len_sq();
                if d_sq < min_dist_sq {
                    min_dist_sq = d_sq;
                    nearest_id = pocket.id;
                }
            }
            dropped_pocket = Some(nearest_id);
        }

        if let Some(pocket) = dropped_pocket {
            b.pocketed = true;
            b.vel = Vec2::ZERO;
            b.spin_xy = Vec2::ZERO;
            b.spin_z = 0.0;
            if b.id == 0 {
                events.push(PhysicsEvent::Scratch { pocket, time_ms });
            } else {
                events.push(PhysicsEvent::Pot {
                    ball: b.id,
                    pocket,
                    time_ms,
                });
            }
        }
    }
}

fn find_earliest_collision(
    balls: &[BallState; BALL_COUNT],
    max_dt: f64,
) -> Option<(f64, CollisionKind)> {
    let mut best_t = max_dt;
    let mut best_kind: Option<CollisionKind> = None;

    find_ball_ball_collision(balls, &mut best_t, &mut best_kind);
    find_rail_and_jaw_collision(balls, max_dt, &mut best_t, &mut best_kind);

    best_kind.map(|kind| (best_t, kind))
}

fn find_ball_ball_collision(
    balls: &[BallState; BALL_COUNT],
    best_t: &mut f64,
    best_kind: &mut Option<CollisionKind>,
) {
    for i in 0..BALL_COUNT {
        let Some(bi) = balls.get(i) else {
            continue;
        };
        if bi.pocketed {
            continue;
        }
        for j in (i + 1)..BALL_COUNT {
            let Some(bj) = balls.get(j) else {
                continue;
            };
            if bj.pocketed {
                continue;
            }
            if let Some(t) =
                swept_circle_toi(bi.pos, bi.vel, bj.pos, bj.vel, BALL_DIAMETER, *best_t)
                && (best_kind.is_none() || t < *best_t)
            {
                *best_t = t;
                *best_kind = Some(CollisionKind::BallBall { i, j });
            }
        }
    }
}

fn find_rail_and_jaw_collision(
    balls: &[BallState; BALL_COUNT],
    max_dt: f64,
    best_t: &mut f64,
    best_kind: &mut Option<CollisionKind>,
) {
    let end_rail_range = CORNER_MOUTH..=(TABLE_HEIGHT - CORNER_MOUTH);
    for (i, b) in balls.iter().enumerate() {
        if b.pocketed {
            continue;
        }
        if b.vel.x < -1e-9 {
            let t = (BALL_RADIUS - b.pos.x) / b.vel.x;
            if (-1e-7..=*best_t).contains(&t) {
                let y_hit = b.pos.y + b.vel.y * clamp(t, 0.0, max_dt);
                if end_rail_range.contains(&y_hit) {
                    let t_clamped = clamp(t, 0.0, max_dt);
                    if best_kind.is_none() || t_clamped < *best_t {
                        *best_t = t_clamped;
                        *best_kind = Some(CollisionKind::Rail {
                            i,
                            rail: RailSide::Left,
                        });
                    }
                }
            }
        }
        if b.vel.x > 1e-9 {
            let t = ((TABLE_WIDTH - BALL_RADIUS) - b.pos.x) / b.vel.x;
            if (-1e-7..=*best_t).contains(&t) {
                let y_hit = b.pos.y + b.vel.y * clamp(t, 0.0, max_dt);
                if end_rail_range.contains(&y_hit) {
                    let t_clamped = clamp(t, 0.0, max_dt);
                    if best_kind.is_none() || t_clamped < *best_t {
                        *best_t = t_clamped;
                        *best_kind = Some(CollisionKind::Rail {
                            i,
                            rail: RailSide::Right,
                        });
                    }
                }
            }
        }
        if b.vel.y < -1e-9 {
            let t = (BALL_RADIUS - b.pos.y) / b.vel.y;
            if (-1e-7..=*best_t).contains(&t) {
                let x_hit = b.pos.x + b.vel.x * clamp(t, 0.0, max_dt);
                if is_on_long_rail_segment(x_hit) {
                    let t_clamped = clamp(t, 0.0, max_dt);
                    if best_kind.is_none() || t_clamped < *best_t {
                        *best_t = t_clamped;
                        *best_kind = Some(CollisionKind::Rail {
                            i,
                            rail: RailSide::Bottom,
                        });
                    }
                }
            }
        }
        if b.vel.y > 1e-9 {
            let t = ((TABLE_HEIGHT - BALL_RADIUS) - b.pos.y) / b.vel.y;
            if (-1e-7..=*best_t).contains(&t) {
                let x_hit = b.pos.x + b.vel.x * clamp(t, 0.0, max_dt);
                if is_on_long_rail_segment(x_hit) {
                    let t_clamped = clamp(t, 0.0, max_dt);
                    if best_kind.is_none() || t_clamped < *best_t {
                        *best_t = t_clamped;
                        *best_kind = Some(CollisionKind::Rail {
                            i,
                            rail: RailSide::Top,
                        });
                    }
                }
            }
        }
        for (jaw_idx, &jaw_pos) in POCKET_JAWS.iter().enumerate() {
            if let Some(t) = swept_circle_toi(
                b.pos,
                b.vel,
                jaw_pos,
                Vec2::ZERO,
                BALL_RADIUS + JAW_RADIUS,
                *best_t,
            ) && (best_kind.is_none() || t < *best_t)
            {
                *best_t = t;
                *best_kind = Some(CollisionKind::Jaw { i, jaw_idx });
            }
        }
    }
}

fn is_on_long_rail_segment(x: f64) -> bool {
    let mid = TABLE_WIDTH * 0.5;
    (x >= CORNER_MOUTH && x <= mid - SIDE_HALF_MOUTH)
        || (x >= mid + SIDE_HALF_MOUTH && x <= TABLE_WIDTH - CORNER_MOUTH)
}

fn swept_circle_toi(
    p1: Vec2,
    v1: Vec2,
    p2: Vec2,
    v2: Vec2,
    min_dist: f64,
    max_t: f64,
) -> Option<f64> {
    let dp = p1 - p2;
    let dv = v1 - v2;
    let approach = dp.dot(dv);
    if approach >= -1e-12 {
        return None;
    }

    let c = dp.len_sq() - min_dist * min_dist;
    if c <= 0.0 {
        // Already touching/slightly overlapping and approaching -> immediate collision.
        return Some(0.0);
    }

    let a = dv.len_sq();
    if a <= 1e-15 {
        return None;
    }
    let disc = approach * approach - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-approach - sqrt(disc)) / a;
    if (0.0..=max_t).contains(&t) {
        Some(t)
    } else {
        None
    }
}

fn resolve_ball_ball(
    balls: &mut [BallState; BALL_COUNT],
    i: usize,
    j: usize,
    time_ms: u32,
    first_contact_recorded: &mut bool,
    events: &mut Vec<PhysicsEvent>,
) {
    let (Some(bi), Some(bj)) = (balls.get(i).copied(), balls.get(j).copied()) else {
        return;
    };
    let delta = bj.pos - bi.pos;
    let Some(normal) = delta.normalized(1e-12) else {
        return;
    };

    let rel_vel = bi.vel - bj.vel;
    let vn = rel_vel.dot(normal);
    if vn <= 0.0 {
        return;
    }

    let impulse = f64::midpoint(1.0, BALL_RESTITUTION) * vn;
    let normal_kick = normal.scale(impulse);

    // Subtle spin-induced tangential throw between balls.
    let tangent = Vec2::new(-normal.y, normal.x);
    let throw_speed = clamp((bi.spin_z - bj.spin_z) * 0.025, -0.18 * vn, 0.18 * vn);
    let throw_kick = tangent.scale(throw_speed);

    let mut next_i = bi;
    let mut next_j = bj;
    next_i.vel = (bi.vel - normal_kick) - throw_kick.scale(0.5);
    next_j.vel = (bj.vel + normal_kick) + throw_kick.scale(0.5);

    // Slight separation nudge if overlapping so CCD progresses strictly forward.
    let dist = delta.len();
    if dist < BALL_DIAMETER {
        let sep = f64::midpoint(BALL_DIAMETER - dist, 1e-7);
        next_i.pos = next_i.pos - normal.scale(sep);
        next_j.pos = next_j.pos + normal.scale(sep);
    }

    if let Some(slot_i) = balls.get_mut(i) {
        *slot_i = next_i;
    }
    if let Some(slot_j) = balls.get_mut(j) {
        *slot_j = next_j;
    }

    if !*first_contact_recorded && (bi.id == 0 || bj.id == 0) {
        let obj = if bi.id == 0 { bj.id } else { bi.id };
        *first_contact_recorded = true;
        events.push(PhysicsEvent::FirstContact { ball: obj, time_ms });
    }
    events.push(PhysicsEvent::BallHit {
        ball_a: bi.id,
        ball_b: bj.id,
        speed: vn,
        time_ms,
    });
}

fn resolve_rail(
    balls: &mut [BallState; BALL_COUNT],
    i: usize,
    rail: RailSide,
    time_ms: u32,
    events: &mut Vec<PhysicsEvent>,
) {
    let Some(b) = balls.get_mut(i) else {
        return;
    };
    let (normal, tangent, impact_speed) = match rail {
        RailSide::Left => {
            b.pos.x = BALL_RADIUS + 1e-7;
            let spd = -b.vel.x;
            b.vel.x = spd * CUSHION_RESTITUTION;
            (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), spd)
        }
        RailSide::Right => {
            b.pos.x = TABLE_WIDTH - BALL_RADIUS - 1e-7;
            let spd = b.vel.x;
            b.vel.x = -spd * CUSHION_RESTITUTION;
            (Vec2::new(-1.0, 0.0), Vec2::new(0.0, -1.0), spd)
        }
        RailSide::Bottom => {
            b.pos.y = BALL_RADIUS + 1e-7;
            let spd = -b.vel.y;
            b.vel.y = spd * CUSHION_RESTITUTION;
            (Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), spd)
        }
        RailSide::Top => {
            b.pos.y = TABLE_HEIGHT - BALL_RADIUS - 1e-7;
            let spd = b.vel.y;
            b.vel.y = -spd * CUSHION_RESTITUTION;
            (Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0), spd)
        }
    };

    // Side English deflects the ball tangentially off the cushion rail.
    let english_kick = b.spin_z * 0.28;
    b.vel = b.vel + tangent.scale(english_kick);
    b.spin_z = canonical_zero(b.spin_z * 0.55);

    // Reflect normal component of rolling spin partially on cushion compression.
    let sn = b.spin_xy.dot(normal);
    b.spin_xy = b.spin_xy - normal.scale(sn * 1.4);

    if impact_speed > 0.01 {
        events.push(PhysicsEvent::CushionHit {
            ball: b.id,
            speed: impact_speed,
            time_ms,
        });
    }
}

fn resolve_jaw(
    balls: &mut [BallState; BALL_COUNT],
    i: usize,
    jaw_idx: usize,
    time_ms: u32,
    events: &mut Vec<PhysicsEvent>,
) {
    let (Some(b), Some(&jaw_pos)) = (balls.get_mut(i), POCKET_JAWS.get(jaw_idx)) else {
        return;
    };
    let delta = b.pos - jaw_pos;
    let Some(normal) = delta.normalized(1e-12) else {
        return;
    };
    let vn = b.vel.dot(normal);
    if vn >= 0.0 {
        return;
    }
    let impact_speed = -vn;
    b.vel = b.vel - normal.scale((1.0 + JAW_RESTITUTION) * vn);
    b.spin_z = canonical_zero(b.spin_z * 0.7);

    let min_dist = BALL_RADIUS + JAW_RADIUS;
    let dist = delta.len();
    if dist < min_dist {
        b.pos = jaw_pos + normal.scale(min_dist + 1e-7);
    }

    if impact_speed > 0.01 {
        events.push(PhysicsEvent::CushionHit {
            ball: b.id,
            speed: impact_speed,
            time_ms,
        });
    }
}

fn step_friction(balls: &mut [BallState; BALL_COUNT], dt: f64) {
    let slide_dv = MU_SLIDE * GRAVITY * dt;
    let roll_dv = MU_ROLL * GRAVITY * dt;
    let side_ds = SIDE_SPIN_DECAY * dt;

    for b in balls {
        if b.pocketed {
            continue;
        }

        // Decay side English.
        let sz = fabs(b.spin_z);
        if sz <= side_ds {
            b.spin_z = 0.0;
        } else {
            b.spin_z = if b.spin_z > 0.0 {
                b.spin_z - side_ds
            } else {
                b.spin_z + side_ds
            };
        }

        // Sliding vs. pure rolling transition.
        let slip = b.vel - b.spin_xy;
        let slip_len = slip.len();

        if slip_len > 3.5 * slide_dv {
            let slip_dir = slip.scale(1.0 / slip_len);
            b.vel = b.vel - slip_dir.scale(slide_dv);
            // Rigid sphere rotational acceleration is (5/2) times linear sliding acceleration.
            b.spin_xy = b.spin_xy + slip_dir.scale(2.5 * slide_dv);
        } else {
            // Converged to pure roll.
            b.spin_xy = b.vel;
            let speed = b.vel.len();
            if speed <= roll_dv {
                b.vel = Vec2::ZERO;
                b.spin_xy = Vec2::ZERO;
            } else {
                let next_vel = b.vel.scale((speed - roll_dv) / speed);
                b.vel = next_vel;
                b.spin_xy = next_vel;
            }
        }

        if !b.is_moving() {
            b.vel = Vec2::ZERO;
            b.spin_xy = Vec2::ZERO;
            b.spin_z = 0.0;
        }
        b.canonicalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::constants::{CENTER_Y, HEAD_STRING_X};

    fn empty_table() -> [BallState; BALL_COUNT] {
        core::array::from_fn(|id| {
            #[expect(clippy::cast_possible_truncation, reason = "id in 0..16")]
            BallState::pocketed(id as u8)
        })
    }

    #[test]
    fn follow_and_draw_spin_produce_opposite_post_impact_cue_travel() {
        let mut table = empty_table();
        table[0] = BallState::stationary(0, Vec2::new(HEAD_STRING_X, CENTER_Y));
        table[1] = BallState::stationary(1, Vec2::new(HEAD_STRING_X + 0.40, CENTER_Y));

        // Moderate speed keeps Ball 1 from rebounding off the far rail back into the cue ball.
        let follow = simulate_shot(
            &table,
            ShotInput {
                angle: 0.0,
                power: 0.34,
                spin_x: 0.0,
                spin_y: 0.9,
                cue_pos: None,
                called_pocket: None,
            },
            false,
        );
        let draw = simulate_shot(
            &table,
            ShotInput {
                angle: 0.0,
                power: 0.34,
                spin_x: 0.0,
                spin_y: -0.9,
                cue_pos: None,
                called_pocket: None,
            },
            false,
        );

        let impact_x = HEAD_STRING_X + 0.40 - BALL_DIAMETER;
        assert!(
            follow.final_balls[0].pos.x > impact_x + 0.12,
            "follow cue ball should roll forward past impact: got {}",
            follow.final_balls[0].pos.x
        );
        assert!(
            draw.final_balls[0].pos.x < impact_x - 0.08,
            "draw cue ball should pull back behind impact: got {}",
            draw.final_balls[0].pos.x
        );
    }

    #[test]
    fn side_english_alters_cushion_rebound_angle() {
        let mut table = empty_table();
        // Aim squarely at the solid top rail (away from the side pocket at x = 1.27).
        table[0] = BallState::stationary(0, Vec2::new(0.85, 0.50));

        let left_eng = simulate_shot(
            &table,
            ShotInput {
                angle: core::f64::consts::FRAC_PI_2,
                power: 0.55,
                spin_x: -0.9,
                spin_y: 0.0,
                cue_pos: None,
                called_pocket: None,
            },
            false,
        );
        let right_eng = simulate_shot(
            &table,
            ShotInput {
                angle: core::f64::consts::FRAC_PI_2,
                power: 0.55,
                spin_x: 0.9,
                spin_y: 0.0,
                cue_pos: None,
                called_pocket: None,
            },
            false,
        );

        assert!(
            (left_eng.final_balls[0].pos.x - right_eng.final_balls[0].pos.x).abs() > 0.15,
            "side English must visibly change rail rebound x: left={}, right={}",
            left_eng.final_balls[0].pos.x,
            right_eng.final_balls[0].pos.x
        );
    }

    #[test]
    fn straight_shot_pots_ball_into_corner_pocket() {
        let mut table = empty_table();
        table[0] = BallState::stationary(0, Vec2::new(2.00, 0.54));
        table[1] = BallState::stationary(1, Vec2::new(2.34, 0.20));

        let aim = crate::math::atan2(0.20 - 0.54, 2.34 - 2.00);
        // Light draw holds the cue ball on the table after potting Ball 1 cleanly.
        let sim = simulate_shot(
            &table,
            ShotInput {
                angle: aim,
                power: 0.45,
                spin_x: 0.0,
                spin_y: -0.35,
                cue_pos: None,
                called_pocket: None,
            },
            true,
        );

        assert!(sim.final_balls[1].pocketed);
        assert!(!sim.final_balls[0].pocketed);
        assert!(sim.keyframes.len() > 2);
    }
}
