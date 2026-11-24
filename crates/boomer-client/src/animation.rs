//! Fixed-stride `f64` animation buffer for zero-GC 60 fps Pool rendering.

use boomer_domain::math::canonical_zero;
use boomer_domain::pool::constants::BALL_COUNT;
use boomer_domain::pool::{BallState, ShotKeyframe, ShotSimulation};

/// Total number of pool balls (cue ball `0` plus object balls `1..=15`).
pub const POOL_BALL_COUNT: usize = BALL_COUNT;

/// Number of `f64` values per ball in the flat frame buffer: `[id, x, y, flags]`.
///
/// `flags`: bit 0 (`1.0`) = active on table, `0.0` = pocketed/hidden.
pub const POOL_BALL_STRIDE: usize = 4;

/// Total length of the flat `Float64Array` returned to JavaScript (`16 * 4 = 64`).
pub const POOL_FRAME_LEN: usize = POOL_BALL_COUNT * POOL_BALL_STRIDE;

/// Stores resting or animating Pool shot keyframes for 60 fps `pool_frame(t_ms)` sampling.
#[derive(Debug, Clone)]
pub struct PoolAnimationBuffer {
    frame: [f64; POOL_FRAME_LEN],
    keyframes: Vec<ShotKeyframe>,
    final_balls: [(f64, f64, bool); POOL_BALL_COUNT],
    duration_ms: f64,
    elapsed_ms: f64,
    playing: bool,
}

impl Default for PoolAnimationBuffer {
    fn default() -> Self {
        Self::new_preview_rack()
    }
}

impl PoolAnimationBuffer {
    /// Initializes a static classic 8-ball preview rack on the `2.54 x 1.27 m` table.
    #[must_use]
    pub fn new_preview_rack() -> Self {
        let mut buf = Self {
            frame: [0.0; POOL_FRAME_LEN],
            keyframes: Vec::new(),
            final_balls: [(0.0, 0.0, false); POOL_BALL_COUNT],
            duration_ms: 0.0,
            elapsed_ms: 0.0,
            playing: false,
        };
        buf.set_ball(0, 0.635, 0.635, true);

        let radius = 0.028_575_f64;
        let dx = radius * 1.732_050_807_568_877_2_f64;
        let dy = radius * 2.0;
        let foot_x = 1.905_f64;
        let center_y = 0.635_f64;

        let rack_ids: [u8; 15] = [1, 9, 2, 10, 8, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15];
        let mut idx = 0_usize;
        for row in 0_u8..5 {
            let row_f = f64::from(row);
            for col in 0..=row {
                if let Some(&ball_id) = rack_ids.get(idx) {
                    let col_f = f64::from(col);
                    let x = foot_x + row_f * dx;
                    let y = center_y + (col_f - row_f * 0.5) * dy;
                    buf.set_ball(ball_id, x, y, true);
                }
                idx += 1;
            }
        }
        buf
    }

    /// Loads static resting ball coordinates (canceling any active playback).
    pub fn load_static(&mut self, balls: &[BallState; POOL_BALL_COUNT]) {
        self.keyframes.clear();
        self.duration_ms = 0.0;
        self.elapsed_ms = 0.0;
        self.playing = false;
        for (i, b) in balls.iter().enumerate() {
            if let Some(slot) = self.final_balls.get_mut(i) {
                *slot = (b.pos.x, b.pos.y, !b.pocketed);
            }
            self.set_ball(b.id, b.pos.x, b.pos.y, !b.pocketed);
        }
    }

    /// Begins 60 fps playback of a deterministic [`ShotSimulation`] and stores authoritative
    /// `snap_balls` to snap cleanly onto when balls come to rest.
    pub fn start_replay(
        &mut self,
        sim: &ShotSimulation,
        snap_balls: &[BallState; POOL_BALL_COUNT],
    ) {
        self.keyframes.clone_from(&sim.keyframes);
        for (i, b) in snap_balls.iter().enumerate() {
            if let Some(slot) = self.final_balls.get_mut(i) {
                *slot = (b.pos.x, b.pos.y, !b.pocketed);
            }
        }
        self.duration_ms = sim.duration_ms;
        self.elapsed_ms = 0.0;
        self.playing = self.duration_ms > 1.0 && !self.keyframes.is_empty();
        if self.playing {
            self.write_interpolated(0.0);
        } else {
            self.snap_to_final();
        }
    }

    /// Returns `true` while balls are still animating.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Advances shot playback by `dt_ms`. Returns `true` on the frame playback finishes.
    pub fn advance(&mut self, dt_ms: u32) -> bool {
        if !self.playing {
            return false;
        }
        self.elapsed_ms += f64::from(dt_ms);
        if self.elapsed_ms >= self.duration_ms {
            self.playing = false;
            self.elapsed_ms = self.duration_ms;
            self.snap_to_final();
            return true;
        }
        self.write_interpolated(self.elapsed_ms);
        false
    }

    /// Updates a single ball slot in the flat frame buffer.
    pub fn set_ball(&mut self, id: u8, x: f64, y: f64, visible: bool) {
        let base = usize::from(id) * POOL_BALL_STRIDE;
        let flags = if visible { 1.0 } else { 0.0 };
        if let Some(slot) = self.frame.get_mut(base..base + POOL_BALL_STRIDE)
            && let [s_id, s_x, s_y, s_flags] = slot
        {
            *s_id = f64::from(id);
            *s_x = canonical_zero(x);
            *s_y = canonical_zero(y);
            *s_flags = flags;
        }
    }

    /// Returns the current 64-element frame buffer (`[id, x, y, flags]` × 16).
    #[must_use]
    pub const fn sample(&self, _t_ms: f64) -> &[f64; POOL_FRAME_LEN] {
        &self.frame
    }

    fn snap_to_final(&mut self) {
        for id in 0..POOL_BALL_COUNT {
            if let Some(&(x, y, vis)) = self.final_balls.get(id) {
                #[expect(clippy::cast_possible_truncation, reason = "id in 0..16")]
                self.set_ball(id as u8, x, y, vis);
            }
        }
    }

    fn write_interpolated(&mut self, t_ms: f64) {
        let Some(&first) = self.keyframes.first() else {
            self.snap_to_final();
            return;
        };
        if t_ms <= first.time_ms || self.keyframes.len() == 1 {
            for id in 0..POOL_BALL_COUNT {
                if let Some(&(x, y, vis)) = first.balls.get(id) {
                    #[expect(clippy::cast_possible_truncation, reason = "id in 0..16")]
                    self.set_ball(id as u8, x, y, vis);
                }
            }
            return;
        }

        // Binary search for the first keyframe with time_ms >= t_ms.
        let idx = self.keyframes.partition_point(|kf| kf.time_ms < t_ms);
        let Some(&next_kf) = self.keyframes.get(idx) else {
            self.snap_to_final();
            return;
        };
        let Some(&prev_kf) = self.keyframes.get(idx.saturating_sub(1)) else {
            self.snap_to_final();
            return;
        };

        let span = next_kf.time_ms - prev_kf.time_ms;
        let alpha = if span > 1e-6 {
            ((t_ms - prev_kf.time_ms) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };

        for id in 0..POOL_BALL_COUNT {
            if let (Some(&(x0, y0, v0)), Some(&(x1, y1, v1))) =
                (prev_kf.balls.get(id), next_kf.balls.get(id))
            {
                let vis = v0 && v1;
                let x = x0 + (x1 - x0) * alpha;
                let y = y0 + (y1 - y0) * alpha;
                #[expect(clippy::cast_possible_truncation, reason = "id in 0..16")]
                self.set_ball(id as u8, x, y, vis);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_rack_populates_all_sixteen_balls() {
        let buf = PoolAnimationBuffer::new_preview_rack();
        let frame = buf.sample(0.0);
        assert_eq!(frame.len(), POOL_FRAME_LEN);
        for id in 0_u8..16 {
            let base = usize::from(id) * POOL_BALL_STRIDE;
            assert!((frame[base] - f64::from(id)).abs() < f64::EPSILON);
            assert!((frame[base + 3] - 1.0).abs() < f64::EPSILON);
        }
    }
}
