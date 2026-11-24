//! Time-sliced deterministic candidate-shot AI for 8-ball pool.
//!
//! Evaluates candidate shots using the exact `boomer-domain::pool::physics`
//! simulator in small per-frame rollout budgets on WASM (`~2-4` rollouts/frame)
//! while running to completion in one loop on the Step 2 server.

use crate::BotLevel;
use crate::math::{Vec2, atan2, clamp};
use crate::pool::constants::{
    BALL_DIAMETER, CENTER_Y, FOOT_SPOT_X, HEAD_STRING_X, POCKET_AIM_TARGETS, TABLE_WIDTH,
};
use crate::pool::physics::ShotInput;
use crate::pool::rules::{PoolGame, PoolOutcome};
use crate::rng::SplitMix64;

/// Progress returned by [`PoolSearch::step`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoolSearchProgress {
    /// More candidate rollouts remain.
    InProgress,
    /// Search finished and chose a perturbed [`ShotInput`].
    Done(ShotInput),
}

/// Resumable candidate-shot search for 8-ball pool.
#[derive(Debug, Clone)]
pub struct PoolSearch {
    game: PoolGame,
    level: BotLevel,
    candidates: Vec<ShotInput>,
    next_index: usize,
    best_shot: Option<ShotInput>,
    best_score: i32,
    rng: SplitMix64,
    finished: bool,
    final_shot: Option<ShotInput>,
}

impl PoolSearch {
    /// Creates a resumable search from the current `game` state.
    #[must_use]
    pub fn new(game: &PoolGame, level: BotLevel, seed: u64) -> Self {
        let candidates = generate_candidates(game, level);
        Self {
            game: game.clone(),
            level,
            candidates,
            next_index: 0,
            best_shot: None,
            best_score: i32::MIN,
            rng: SplitMix64::new(seed),
            finished: false,
            final_shot: None,
        }
    }

    /// Runs all candidate rollouts synchronously (for Step 2 server worker pool).
    #[must_use]
    pub fn run_to_completion(&mut self) -> ShotInput {
        loop {
            match self.step(u32::MAX) {
                PoolSearchProgress::InProgress => {}
                PoolSearchProgress::Done(shot) => return shot,
            }
        }
    }

    /// Evaluates up to `rollout_budget` candidate shots via the exact physics simulator.
    pub fn step(&mut self, rollout_budget: u32) -> PoolSearchProgress {
        if self.finished
            && let Some(shot) = self.final_shot
        {
            return PoolSearchProgress::Done(shot);
        }

        let mut evaluated = 0_u32;
        while evaluated < rollout_budget
            && let Some(&candidate) = self.candidates.get(self.next_index)
        {
            let score = evaluate_candidate(&self.game, candidate);
            if score > self.best_score {
                self.best_score = score;
                self.best_shot = Some(candidate);
            }
            self.next_index += 1;
            evaluated += 1;
        }

        if self.next_index < self.candidates.len() {
            return PoolSearchProgress::InProgress;
        }

        let raw = self.best_shot.unwrap_or(ShotInput {
            angle: 0.0,
            power: 0.45,
            spin_x: 0.0,
            spin_y: 0.0,
            cue_pos: None,
            called_pocket: None,
        });
        let perturbed = self.apply_level_error(raw);
        self.final_shot = Some(perturbed);
        self.finished = true;
        PoolSearchProgress::Done(perturbed)
    }

    fn apply_level_error(&mut self, mut shot: ShotInput) -> ShotInput {
        let (angle_err, power_err) = match self.level {
            BotLevel::Easy => (0.055, 0.14),
            BotLevel::Medium => (0.018, 0.06),
            BotLevel::Hard => (0.004, 0.02),
        };
        let a_delta = (self.rng.next_unit_f64() * 2.0 - 1.0) * angle_err;
        let p_delta = (self.rng.next_unit_f64() * 2.0 - 1.0) * power_err;
        shot.angle += a_delta;
        shot.power = clamp(shot.power + p_delta, 0.08, 1.0);
        if self.level == BotLevel::Easy {
            shot.spin_x = 0.0;
            shot.spin_y = 0.0;
        }
        shot
    }
}

fn generate_candidates(game: &PoolGame, level: BotLevel) -> Vec<ShotInput> {
    let mut out = Vec::new();
    let balls = game.balls();

    // Opening break candidates.
    if game.is_break_shot() {
        let break_pos = Vec2::new(HEAD_STRING_X, CENTER_Y);
        let aim = atan2(CENTER_Y - break_pos.y, FOOT_SPOT_X - break_pos.x);
        for offset in [-0.015_f64, 0.0, 0.015] {
            out.push(ShotInput {
                angle: aim + offset,
                power: 0.96,
                spin_x: 0.0,
                spin_y: 0.2,
                cue_pos: if game.ball_in_hand() {
                    Some(break_pos)
                } else {
                    None
                },
                called_pocket: None,
            });
        }
        return out;
    }

    let targets = game.legal_target_balls();
    let current_cue = balls
        .first()
        .map_or(Vec2::new(HEAD_STRING_X, CENTER_Y), |b| b.pos);
    let max_cut_cos = 0.22_f64; // ~77 deg max cut angle

    let powers: &[f64] = match level {
        BotLevel::Easy => &[0.50],
        BotLevel::Medium => &[0.46, 0.68],
        BotLevel::Hard => &[0.42, 0.60, 0.78],
    };

    for &target_id in &targets {
        let Some(target) = balls.get(usize::from(target_id)) else {
            continue;
        };
        if target.pocketed {
            continue;
        }

        for (pocket_idx, &pocket_pos) in POCKET_AIM_TARGETS.iter().enumerate() {
            let Some(to_pocket) = (pocket_pos - target.pos).normalized(1e-6) else {
                continue;
            };
            let ghost_pos = target.pos - to_pocket.scale(BALL_DIAMETER);

            // If AI has ball-in-hand, synthesize a straight aligned placement behind the ghost ball.
            let (cue_pos, explicit_cue) = if game.ball_in_hand() {
                if let Some(placed) = find_bih_placement(game, ghost_pos, to_pocket) {
                    (placed, Some(placed))
                } else {
                    (current_cue, None)
                }
            } else {
                (current_cue, None)
            };

            let Some(cue_to_ghost) = (ghost_pos - cue_pos).normalized(1e-6) else {
                continue;
            };
            let cut_cos = cue_to_ghost.dot(to_pocket);
            if cut_cos < max_cut_cos {
                continue;
            }

            let angle = atan2(cue_to_ghost.y, cue_to_ghost.x);
            #[expect(clippy::cast_possible_truncation, reason = "pocket_idx in 0..6")]
            let pocket_u8 = pocket_idx as u8;
            for &power in powers {
                out.push(ShotInput {
                    angle,
                    power,
                    spin_x: 0.0,
                    spin_y: if level == BotLevel::Hard && power < 0.55 {
                        0.25
                    } else {
                        0.0
                    },
                    cue_pos: explicit_cue,
                    called_pocket: Some(pocket_u8),
                });
            }
        }
    }

    // Add direct safety/contact fallbacks aimed at legal target balls in case pockets are blocked.
    for &target_id in targets.iter().take(4) {
        if let Some(target) = balls.get(usize::from(target_id))
            && !target.pocketed
        {
            let dir = target.pos - current_cue;
            let angle = atan2(dir.y, dir.x);
            out.push(ShotInput {
                angle,
                power: 0.42,
                spin_x: 0.0,
                spin_y: 0.0,
                cue_pos: None,
                called_pocket: Some(0),
            });
        }
    }

    out
}

fn find_bih_placement(game: &PoolGame, ghost_pos: Vec2, to_pocket: Vec2) -> Option<Vec2> {
    for dist in [0.35_f64, 0.50, 0.25, 0.65] {
        let candidate = ghost_pos - to_pocket.scale(dist);
        if game.is_valid_cue_placement(candidate) {
            return Some(candidate);
        }
    }
    None
}

fn evaluate_candidate(game: &PoolGame, shot: ShotInput) -> i32 {
    let mut sim_game = game.clone();
    let shooter = sim_game.active_seat();
    let Ok(outcome) = sim_game.shoot(shot, false) else {
        return -10_000;
    };

    if let Some(PoolOutcome { winner, .. }) = sim_game.outcome() {
        return if winner == shooter { 5_000 } else { -5_000 };
    }

    let mut score = 0_i32;
    if !outcome.fouls.is_empty() {
        score -= 800;
    }
    if outcome.keeps_turn {
        score += 450;
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "potted <= 15"
    )]
    let pot_count = outcome.potted.len() as i32;
    score += pot_count * 120;

    // Reward cue-ball resting near center of table for good next-shot position.
    if let Some(cue) = sim_game.balls().first()
        && !cue.pocketed
    {
        let center = Vec2::new(TABLE_WIDTH * 0.5, CENTER_Y);
        let dist = cue.pos.dist(center);
        #[expect(clippy::cast_possible_truncation, reason = "dist in [0, 1.5]")]
        let pos_penalty = (dist * 35.0) as i32;
        score -= pos_penalty;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Seat;
    use crate::pool::rules::PoolSettings;

    #[test]
    fn time_sliced_pool_ai_matches_full_search_identically() {
        let game = PoolGame::new(Seat::One, PoolSettings::default(), false, 42);
        let mut full = PoolSearch::new(&game, BotLevel::Hard, 7);
        let full_shot = full.run_to_completion();

        let mut sliced = PoolSearch::new(&game, BotLevel::Hard, 7);
        let sliced_shot = loop {
            match sliced.step(1) {
                PoolSearchProgress::InProgress => {}
                PoolSearchProgress::Done(s) => break s,
            }
        };

        assert_eq!(full_shot.angle.to_bits(), sliced_shot.angle.to_bits());
        assert_eq!(full_shot.power.to_bits(), sliced_shot.power.to_bits());
    }

    #[test]
    fn medium_pool_ai_executes_legal_break_shot() {
        let mut game = PoolGame::new(Seat::One, PoolSettings::default(), false, 1);
        let mut search = PoolSearch::new(&game, BotLevel::Medium, 11);
        let shot = search.run_to_completion();
        assert!(shot.power >= 0.08 && shot.power <= 1.0);
        let _ = game.shoot(shot, false).unwrap();
    }
}
