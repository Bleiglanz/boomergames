//! Pure Rust aiming-aid raycaster (`Off`, `Basic`, `Extended`).

use crate::math::{Vec2, cos, sin, sqrt};
use crate::pool::constants::{BALL_COUNT, BALL_DIAMETER, BALL_RADIUS, TABLE_HEIGHT, TABLE_WIDTH};
use crate::pool::physics::BallState;

/// Configurable aiming-aid level chosen by the player / table host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AimAidLevel {
    /// Cue direction stub only.
    Off,
    /// Aim line to first contact plus ghost-ball position.
    Basic,
    /// Ghost ball plus predicted object-ball travel line and cue-ball tangent line.
    Extended,
}

/// Geometry projected to the renderer so TypeScript computes zero pool math.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimPreview {
    /// Origin of the aim line (cue ball center).
    pub cue_pos: Vec2,
    /// End of the primary aim line (ghost-ball contact center, or short stub when `Off`).
    pub ghost_ball: Vec2,
    /// First contacted object ball ID, if any (and `level != Off`).
    pub target_ball: Option<u8>,
    /// Start and end of predicted object-ball deflection vector (`Extended` only).
    pub object_line: Option<(Vec2, Vec2)>,
    /// Start and end of predicted cue-ball deflection tangent (`Extended` only).
    pub cue_deflection_line: Option<(Vec2, Vec2)>,
}

/// Computes the deterministic aiming aid geometry for a cue strike at `angle`.
#[must_use]
pub fn compute_aim_preview(
    balls: &[BallState; BALL_COUNT],
    cue_pos: Vec2,
    angle: f64,
    level: AimAidLevel,
) -> AimPreview {
    let dir = Vec2::new(cos(angle), sin(angle));
    if level == AimAidLevel::Off {
        return AimPreview {
            cue_pos,
            ghost_ball: cue_pos + dir.scale(0.28),
            target_ball: None,
            object_line: None,
            cue_deflection_line: None,
        };
    }

    // Distance to inner rail rectangle `[R, W - R] x [R, H - R]`.
    let mut best_t = ray_box_distance(cue_pos, dir);
    let mut hit_ball: Option<(u8, Vec2)> = None;

    for b in balls.iter().skip(1) {
        if b.pocketed {
            continue;
        }
        if let Some(t) = ray_circle_distance(cue_pos, dir, b.pos, BALL_DIAMETER)
            && t < best_t
        {
            best_t = t;
            hit_ball = Some((b.id, b.pos));
        }
    }

    let ghost_ball = cue_pos + dir.scale(best_t);

    if level == AimAidLevel::Basic {
        return AimPreview {
            cue_pos,
            ghost_ball,
            target_ball: hit_ball.map(|(id, _)| id),
            object_line: None,
            cue_deflection_line: None,
        };
    }

    // Extended: compute predicted object-ball line and cue-ball tangent line.
    if let Some((ball_id, target_pos)) = hit_ball
        && let Some(impact_normal) = (target_pos - ghost_ball).normalized(1e-9)
    {
        let obj_end = target_pos + impact_normal.scale(0.55);
        let align = dir.dot(impact_normal);
        let tangent_raw = dir - impact_normal.scale(align);
        let cue_deflection_line = tangent_raw
            .normalized(1e-4)
            .map(|t_dir| (ghost_ball, ghost_ball + t_dir.scale(0.32)));

        return AimPreview {
            cue_pos,
            ghost_ball,
            target_ball: Some(ball_id),
            object_line: Some((target_pos, obj_end)),
            cue_deflection_line,
        };
    }

    AimPreview {
        cue_pos,
        ghost_ball,
        target_ball: None,
        object_line: None,
        cue_deflection_line: None,
    }
}

fn ray_box_distance(origin: Vec2, dir: Vec2) -> f64 {
    let mut best = 4.0_f64;
    if dir.x < -1e-9 {
        let t = (BALL_RADIUS - origin.x) / dir.x;
        if t >= 0.0 && t < best {
            best = t;
        }
    } else if dir.x > 1e-9 {
        let t = ((TABLE_WIDTH - BALL_RADIUS) - origin.x) / dir.x;
        if t >= 0.0 && t < best {
            best = t;
        }
    }
    if dir.y < -1e-9 {
        let t = (BALL_RADIUS - origin.y) / dir.y;
        if t >= 0.0 && t < best {
            best = t;
        }
    } else if dir.y > 1e-9 {
        let t = ((TABLE_HEIGHT - BALL_RADIUS) - origin.y) / dir.y;
        if t >= 0.0 && t < best {
            best = t;
        }
    }
    best
}

fn ray_circle_distance(origin: Vec2, dir: Vec2, center: Vec2, radius: f64) -> Option<f64> {
    let oc = origin - center;
    let b = oc.dot(dir);
    if b >= 0.0 {
        return None;
    }
    let c = oc.len_sq() - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - sqrt(disc);
    if t >= 0.0 { Some(t) } else { None }
}
