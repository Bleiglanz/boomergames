//! Fixed-stride `f64` animation buffer for zero-GC 60 fps Pool rendering.

use boomer_domain::math::canonical_zero;

/// Total number of pool balls (cue ball `0` plus object balls `1..=15`).
pub const POOL_BALL_COUNT: usize = 16;

/// Number of `f64` values per ball in the flat frame buffer: `[id, x, y, flags]`.
///
/// `flags`: bit 0 (`1.0`) = active on table, `0.0` = pocketed/hidden.
pub const POOL_BALL_STRIDE: usize = 4;

/// Total length of the flat `Float64Array` returned to JavaScript (`16 * 4 = 64`).
pub const POOL_FRAME_LEN: usize = POOL_BALL_COUNT * POOL_BALL_STRIDE;

/// Stores current ball positions for 60 fps `pool_frame(t_ms)` sampling.
#[derive(Debug, Clone)]
pub struct PoolAnimationBuffer {
    frame: [f64; POOL_FRAME_LEN],
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
        };
        // Cue ball on the head string (0.635, 0.635).
        buf.set_ball(0, 0.635, 0.635, true);

        // Standard 5-row triangle rack anchored at foot spot (1.905, 0.635).
        let radius = 0.028_575_f64;
        let dx = radius * 1.732_050_807_568_877_2_f64;
        let dy = radius * 2.0;
        let foot_x = 1.905_f64;
        let center_y = 0.635_f64;

        // Classic order putting the 8-ball in the center (row 2, col 1) and solid/stripe corners.
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

    /// Samples the interpolated ball buffer at time `t_ms`.
    #[must_use]
    pub fn sample(&self, _t_ms: f64) -> &[f64; POOL_FRAME_LEN] {
        &self.frame
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
