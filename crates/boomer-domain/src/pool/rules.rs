//! Strict WPA/BCA 8-ball pool ruleset and match state machine.

use crate::Seat;
use crate::math::{Vec2, sqrt};
use crate::pool::constants::{
    BALL_COUNT, BALL_DIAMETER, BALL_RADIUS, CENTER_Y, FOOT_SPOT_X, HEAD_STRING_X, TABLE_HEIGHT,
    TABLE_WIDTH,
};
use crate::pool::physics::{BallState, PhysicsEvent, ShotInput, ShotSimulation, simulate_shot};
use crate::rng::SplitMix64;

/// Ball group assignment in 8-ball pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BallGroup {
    /// Solid balls `1..=7`.
    Solids,
    /// Striped balls `9..=15`.
    Stripes,
}

impl BallGroup {
    /// Returns the opposing ball group.
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Self::Solids => Self::Stripes,
            Self::Stripes => Self::Solids,
        }
    }

    /// Returns the group of an object ball (`1..=7` -> Solids, `9..=15` -> Stripes, `0` or `8` -> `None`).
    #[must_use]
    pub const fn of_ball(ball_id: u8) -> Option<Self> {
        match ball_id {
            1..=7 => Some(Self::Solids),
            9..=15 => Some(Self::Stripes),
            _ => None,
        }
    }
}

/// Ball-in-hand placement scope after a foul.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BallInHandRule {
    /// Cue ball may be placed anywhere on the playing surface (WPA standard).
    Anywhere,
    /// Cue ball must be placed behind the head string (`x <= 0.635 m`).
    BehindHeadString,
}

/// Break rule when the 8-ball is legally pocketed on the opening break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EightOnBreakRule {
    /// Re-rack and break again.
    ReRack,
    /// Instant win for the breaker.
    Win,
}

/// Configurable 8-ball pool match rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolSettings {
    /// Aiming aid level (`Off`, `Basic`, `Extended`).
    pub aim_aid: crate::pool::aim::AimAidLevel,
    /// Whether the shooter must call a pocket before sinking the 8-ball.
    pub call_eight: bool,
    /// Ball-in-hand placement rule after fouls.
    pub ball_in_hand: BallInHandRule,
    /// Behavior when the 8-ball drops on a legal break.
    pub eight_on_break: EightOnBreakRule,
    /// Shot clock in seconds (`0` = unlimited).
    pub shot_timer_secs: u32,
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            aim_aid: crate::pool::aim::AimAidLevel::Extended,
            call_eight: false,
            ball_in_hand: BallInHandRule::Anywhere,
            eight_on_break: EightOnBreakRule::ReRack,
            shot_timer_secs: 30,
        }
    }
}

/// Fouls recognized in 2D WPA/BCA 8-ball pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolFoul {
    /// Cue ball pocketed.
    Scratch,
    /// First ball contacted was not a legal target (including hitting 8-ball first on an open table).
    WrongFirstBall,
    /// Cue ball failed to strike any object ball.
    NoHit,
    /// No ball was pocketed and no ball hit a cushion rail after first contact.
    NoRailAfterContact,
}

/// Reason a player won an 8-ball match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolWinReason {
    /// Winner legally pocketed the 8-ball after clearing their group.
    EightBallPocketed,
    /// Opponent pocketed the 8-ball early before clearing their group.
    OpponentEarlyEight,
    /// Opponent pocketed the 8-ball on a foul or wrong called pocket.
    OpponentFoulOnEight,
    /// Opponent resigned.
    Resign,
}

/// Terminal outcome of an 8-ball match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolOutcome {
    /// Winning seat.
    pub winner: Seat,
    /// Reason for victory.
    pub reason: PoolWinReason,
}

/// Errors returned by [`PoolGame`] operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolError {
    /// Match has already ended.
    GameOver,
    /// Shot parameter is non-finite or out of legal bounds.
    InvalidShot,
    /// Cue ball placement overlaps another ball or violates bounds/head-string rules.
    InvalidCuePlacement,
}

/// Result of executing one shot in [`PoolGame`].
#[derive(Debug, Clone, PartialEq)]
pub struct ShotOutcome {
    /// Full deterministic physics simulation (final balls, events, keyframes).
    pub simulation: ShotSimulation,
    /// Fouls committed on this shot (empty if legal).
    pub fouls: Vec<PoolFoul>,
    /// Object balls pocketed during this shot (`(ball_id, pocket_id)`).
    pub potted: Vec<(u8, u8)>,
    /// Whether the rack was automatically re-racked (8-ball on break with `ReRack` rule).
    pub reracked: bool,
    /// Whether the active shooter retains their turn.
    pub keeps_turn: bool,
}

/// Cue-ball placement permission state (eliminates invalid bool combinations).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CuePlacement {
    Fixed,
    Anywhere,
    BehindHeadString,
}

/// Extracted event summary for one simulated pool shot.
struct ShotEventSummary {
    first_contact: Option<u8>,
    scratched: bool,
    potted: Vec<(u8, u8)>,
    rail_after_contact: bool,
    break_rail_count: usize,
}

/// Authoritative 8-ball pool match state.
#[derive(Debug, Clone)]
pub struct PoolGame {
    balls: [BallState; BALL_COUNT],
    active_seat: Seat,
    groups: [Option<BallGroup>; 2],
    pocketed_by_seat: [Vec<u8>; 2],
    is_break_shot: bool,
    cue_placement: CuePlacement,
    turn_number: u32,
    settings: PoolSettings,
    free_practice: bool,
    outcome: Option<PoolOutcome>,
    rng: SplitMix64,
}

impl PoolGame {
    /// Starts a new 8-ball rack with deterministic `seed`.
    #[must_use]
    pub fn new(breaker: Seat, settings: PoolSettings, free_practice: bool, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let balls = build_triangle_rack(&mut rng);
        Self {
            balls,
            active_seat: breaker,
            groups: [None, None],
            pocketed_by_seat: [Vec::new(), Vec::new()],
            is_break_shot: true,
            cue_placement: CuePlacement::BehindHeadString,
            turn_number: 1,
            settings,
            free_practice,
            outcome: None,
            rng,
        }
    }

    /// Re-racks the table cleanly (used in Free Practice or 8-on-break re-rack).
    pub fn rerack(&mut self, breaker: Seat) {
        self.balls = build_triangle_rack(&mut self.rng);
        self.active_seat = breaker;
        self.groups = [None, None];
        self.pocketed_by_seat = [Vec::new(), Vec::new()];
        self.is_break_shot = true;
        self.cue_placement = CuePlacement::BehindHeadString;
        self.outcome = None;
    }

    /// Current ball states (`0..16`).
    #[must_use]
    pub const fn balls(&self) -> &[BallState; BALL_COUNT] {
        &self.balls
    }

    /// Active shooter seat.
    #[must_use]
    pub const fn active_seat(&self) -> Seat {
        self.active_seat
    }

    /// Assigned ball groups for `[Seat::One, Seat::Two]` (`None` while table is open).
    #[must_use]
    pub const fn groups(&self) -> [Option<BallGroup>; 2] {
        self.groups
    }

    /// Pocketed ball numbers attributed to `[Seat::One, Seat::Two]`.
    #[must_use]
    pub const fn pocketed_by_seat(&self) -> &[Vec<u8>; 2] {
        &self.pocketed_by_seat
    }

    /// Whether the next shot is the opening break.
    #[must_use]
    pub const fn is_break_shot(&self) -> bool {
        self.is_break_shot
    }

    /// Whether the active player currently has ball-in-hand.
    #[must_use]
    pub const fn ball_in_hand(&self) -> bool {
        !matches!(self.cue_placement, CuePlacement::Fixed)
    }

    /// Whether ball-in-hand placement is restricted behind the head string.
    #[must_use]
    pub const fn ball_in_hand_behind_line(&self) -> bool {
        matches!(self.cue_placement, CuePlacement::BehindHeadString)
    }

    /// Monotonic turn counter.
    #[must_use]
    pub const fn turn_number(&self) -> u32 {
        self.turn_number
    }

    /// Active pool settings.
    #[must_use]
    pub const fn settings(&self) -> PoolSettings {
        self.settings
    }

    /// Updates table settings (applies immediately).
    pub fn set_settings(&mut self, settings: PoolSettings) {
        self.settings = settings;
    }

    /// Whether this table is in solo Free Practice mode.
    #[must_use]
    pub const fn is_free_practice(&self) -> bool {
        self.free_practice
    }

    /// Terminal outcome, or `None` while the rack is active.
    #[must_use]
    pub const fn outcome(&self) -> Option<PoolOutcome> {
        self.outcome
    }

    /// Returns the ball group assigned to `seat`, or `None` if the table is open.
    #[must_use]
    pub const fn group_of(&self, seat: Seat) -> Option<BallGroup> {
        let [g1, g2] = self.groups;
        match seat {
            Seat::One => g1,
            Seat::Two => g2,
        }
    }

    /// Returns `true` if `seat` has cleared all 7 balls of its assigned group and is now shooting for the 8-ball.
    #[must_use]
    pub fn is_on_eight_ball(&self, seat: Seat) -> bool {
        let Some(group) = self.group_of(seat) else {
            return false;
        };
        !self
            .balls
            .iter()
            .any(|b| !b.pocketed && BallGroup::of_ball(b.id) == Some(group))
    }

    /// Returns the list of legal target object-ball IDs for the active shooter.
    #[must_use]
    pub fn legal_target_balls(&self) -> Vec<u8> {
        if self.free_practice {
            return self
                .balls
                .iter()
                .filter(|b| b.id != 0 && !b.pocketed)
                .map(|b| b.id)
                .collect();
        }
        match self.group_of(self.active_seat) {
            None => self
                .balls
                .iter()
                .filter(|b| !b.pocketed && BallGroup::of_ball(b.id).is_some())
                .map(|b| b.id)
                .collect(),
            Some(group) => {
                let own: Vec<u8> = self
                    .balls
                    .iter()
                    .filter(|b| !b.pocketed && BallGroup::of_ball(b.id) == Some(group))
                    .map(|b| b.id)
                    .collect();
                if own.is_empty() {
                    if self.balls.get(8).is_some_and(|b| !b.pocketed) {
                        vec![8]
                    } else {
                        Vec::new()
                    }
                } else {
                    own
                }
            }
        }
    }

    /// Validates whether `pos` is a legal cue-ball placement right now.
    #[must_use]
    pub fn is_valid_cue_placement(&self, pos: Vec2) -> bool {
        if !pos.x.is_finite() || !pos.y.is_finite() {
            return false;
        }
        let min_x = BALL_RADIUS + 0.002;
        let max_x = if self.ball_in_hand_behind_line() {
            HEAD_STRING_X
        } else {
            TABLE_WIDTH - BALL_RADIUS - 0.002
        };
        let min_y = BALL_RADIUS + 0.002;
        let max_y = TABLE_HEIGHT - BALL_RADIUS - 0.002;
        if pos.x < min_x || pos.x > max_x || pos.y < min_y || pos.y > max_y {
            return false;
        }
        let min_sep_sq = (BALL_DIAMETER + 0.001) * (BALL_DIAMETER + 0.001);
        for b in self.balls.iter().skip(1) {
            if !b.pocketed && (b.pos - pos).len_sq() < min_sep_sq {
                return false;
            }
        }
        true
    }

    /// Moves the cue ball to `pos` while `ball_in_hand` is active.
    ///
    /// # Errors
    /// Returns [`PoolError::InvalidCuePlacement`] if placement is illegal.
    pub fn place_cue_ball(&mut self, pos: Vec2) -> Result<(), PoolError> {
        if self.outcome.is_some() {
            return Err(PoolError::GameOver);
        }
        if !self.ball_in_hand() || !self.is_valid_cue_placement(pos) {
            return Err(PoolError::InvalidCuePlacement);
        }
        if let Some(cue) = self.balls.get_mut(0) {
            cue.pos = Vec2::new(pos.x, pos.y);
            cue.pocketed = false;
        }
        Ok(())
    }

    /// Applies shot clock expiry: passes turn to the opponent with ball-in-hand (never forfeits match).
    ///
    /// # Errors
    /// Returns [`PoolError::GameOver`] if the match already ended.
    pub fn timeout_turn(&mut self) -> Result<(), PoolError> {
        if self.outcome.is_some() {
            return Err(PoolError::GameOver);
        }
        if self.free_practice {
            return Ok(());
        }
        self.active_seat = self.active_seat.opponent();
        self.cue_placement = self.foul_cue_placement();
        self.is_break_shot = false;
        self.turn_number = self.turn_number.saturating_add(1);
        Ok(())
    }

    /// Ends the match by resignation of `loser`.
    ///
    /// # Errors
    /// Returns [`PoolError::GameOver`] if already finished.
    pub fn resign(&mut self, loser: Seat) -> Result<(), PoolError> {
        if self.outcome.is_some() {
            return Err(PoolError::GameOver);
        }
        self.outcome = Some(PoolOutcome {
            winner: loser.opponent(),
            reason: PoolWinReason::Resign,
        });
        Ok(())
    }

    /// Executes `shot`, updates authoritative table state per strict WPA/BCA 8-ball rules,
    /// and returns the simulation + rule outcome.
    ///
    /// # Errors
    /// Returns [`PoolError`] if the match is over or shot/placement parameters are invalid.
    pub fn shoot(
        &mut self,
        shot: ShotInput,
        record_keyframes: bool,
    ) -> Result<ShotOutcome, PoolError> {
        if self.outcome.is_some() {
            return Err(PoolError::GameOver);
        }
        if !shot.angle.is_finite()
            || !shot.power.is_finite()
            || !shot.spin_x.is_finite()
            || !shot.spin_y.is_finite()
            || !(0.02..=1.0).contains(&shot.power)
            || !(-1.0..=1.0).contains(&shot.spin_x)
            || !(-1.0..=1.0).contains(&shot.spin_y)
        {
            return Err(PoolError::InvalidShot);
        }
        if let Some(pocket) = shot.called_pocket
            && pocket >= 6
        {
            return Err(PoolError::InvalidShot);
        }
        if let Some(cue_pos) = shot.cue_pos {
            if !self.ball_in_hand() || !self.is_valid_cue_placement(cue_pos) {
                return Err(PoolError::InvalidCuePlacement);
            }
            if let Some(cue) = self.balls.get_mut(0) {
                cue.pos = Vec2::new(cue_pos.x, cue_pos.y);
                cue.pocketed = false;
            }
        }

        let sim = simulate_shot(&self.balls, shot, record_keyframes);
        let outcome = self.apply_simulation(sim, shot.called_pocket);
        Ok(outcome)
    }

    const fn foul_cue_placement(&self) -> CuePlacement {
        match self.settings.ball_in_hand {
            BallInHandRule::Anywhere => CuePlacement::Anywhere,
            BallInHandRule::BehindHeadString => CuePlacement::BehindHeadString,
        }
    }

    fn apply_simulation(&mut self, sim: ShotSimulation, called_pocket: Option<u8>) -> ShotOutcome {
        let summary = summarize_events(&sim.events);
        self.balls = sim.final_balls;
        let shooter = self.active_seat;
        let was_break = self.is_break_shot;
        let was_on_eight = self.is_on_eight_ball(shooter);

        if self.free_practice {
            return self.apply_free_practice(sim, summary, shooter);
        }

        let fouls = self.evaluate_fouls(&summary, shooter, was_break, was_on_eight);
        let has_foul = !fouls.is_empty();
        let eight_pot = summary.potted.iter().find(|&&(ball, _)| ball == 8).copied();

        if was_break && eight_pot.is_some() {
            return self.resolve_eight_on_break(sim, fouls, summary.potted, shooter);
        }
        if let Some((_, eight_pocket)) = eight_pot {
            return self.resolve_eight_pot(
                sim,
                fouls,
                summary.potted,
                summary.scratched,
                shooter,
                was_on_eight,
                called_pocket,
                eight_pocket,
            );
        }

        self.resolve_standard_turn(sim, fouls, summary, shooter, was_break, has_foul)
    }

    fn apply_free_practice(
        &mut self,
        sim: ShotSimulation,
        summary: ShotEventSummary,
        shooter: Seat,
    ) -> ShotOutcome {
        if summary.scratched {
            self.respawn_cue_ball(CuePlacement::Anywhere);
        }
        for &(ball, _) in &summary.potted {
            self.record_pocketed_ball(ball, shooter);
        }
        self.is_break_shot = false;
        self.cue_placement = if summary.scratched {
            CuePlacement::Anywhere
        } else {
            CuePlacement::Fixed
        };
        self.turn_number = self.turn_number.saturating_add(1);
        let fouls = if summary.scratched {
            vec![PoolFoul::Scratch]
        } else {
            Vec::new()
        };
        ShotOutcome {
            simulation: sim,
            fouls,
            potted: summary.potted,
            reracked: false,
            keeps_turn: true,
        }
    }

    fn evaluate_fouls(
        &self,
        summary: &ShotEventSummary,
        shooter: Seat,
        was_break: bool,
        was_on_eight: bool,
    ) -> Vec<PoolFoul> {
        let mut fouls = Vec::new();
        if summary.scratched {
            fouls.push(PoolFoul::Scratch);
        }
        match summary.first_contact {
            None => fouls.push(PoolFoul::NoHit),
            Some(hit_ball) if !was_break => {
                let legal_first = match self.group_of(shooter) {
                    None => BallGroup::of_ball(hit_ball).is_some(),
                    Some(group) => {
                        if was_on_eight {
                            hit_ball == 8
                        } else {
                            BallGroup::of_ball(hit_ball) == Some(group)
                        }
                    }
                };
                if !legal_first {
                    fouls.push(PoolFoul::WrongFirstBall);
                }
            }
            Some(_) => {}
        }

        if was_break {
            if summary.potted.is_empty()
                && summary.break_rail_count < 4
                && !fouls.contains(&PoolFoul::NoHit)
            {
                fouls.push(PoolFoul::NoRailAfterContact);
            }
        } else if summary.potted.is_empty()
            && summary.first_contact.is_some()
            && !summary.rail_after_contact
        {
            fouls.push(PoolFoul::NoRailAfterContact);
        }
        fouls
    }

    fn resolve_eight_on_break(
        &mut self,
        sim: ShotSimulation,
        fouls: Vec<PoolFoul>,
        potted: Vec<(u8, u8)>,
        shooter: Seat,
    ) -> ShotOutcome {
        let has_foul = !fouls.is_empty();
        if !has_foul && self.settings.eight_on_break == EightOnBreakRule::Win {
            self.outcome = Some(PoolOutcome {
                winner: shooter,
                reason: PoolWinReason::EightBallPocketed,
            });
            self.turn_number = self.turn_number.saturating_add(1);
            return ShotOutcome {
                simulation: sim,
                fouls,
                potted,
                reracked: false,
                keeps_turn: false,
            };
        }
        let next_breaker = if has_foul {
            shooter.opponent()
        } else {
            shooter
        };
        self.rerack(next_breaker);
        self.turn_number = self.turn_number.saturating_add(1);
        ShotOutcome {
            simulation: sim,
            fouls,
            potted,
            reracked: true,
            keeps_turn: !has_foul,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Encapsulates terminal 8-ball resolution cleanly without bloating apply_simulation"
    )]
    fn resolve_eight_pot(
        &mut self,
        sim: ShotSimulation,
        fouls: Vec<PoolFoul>,
        potted: Vec<(u8, u8)>,
        scratched: bool,
        shooter: Seat,
        was_on_eight: bool,
        called_pocket: Option<u8>,
        eight_pocket: u8,
    ) -> ShotOutcome {
        let has_foul = !fouls.is_empty();
        if !was_on_eight {
            self.outcome = Some(PoolOutcome {
                winner: shooter.opponent(),
                reason: PoolWinReason::OpponentEarlyEight,
            });
        } else if has_foul || (self.settings.call_eight && called_pocket != Some(eight_pocket)) {
            self.outcome = Some(PoolOutcome {
                winner: shooter.opponent(),
                reason: PoolWinReason::OpponentFoulOnEight,
            });
        } else {
            self.outcome = Some(PoolOutcome {
                winner: shooter,
                reason: PoolWinReason::EightBallPocketed,
            });
        }
        if scratched {
            self.respawn_cue_ball(CuePlacement::Anywhere);
        }
        self.turn_number = self.turn_number.saturating_add(1);
        ShotOutcome {
            simulation: sim,
            fouls,
            potted,
            reracked: false,
            keeps_turn: false,
        }
    }

    fn resolve_standard_turn(
        &mut self,
        sim: ShotSimulation,
        fouls: Vec<PoolFoul>,
        summary: ShotEventSummary,
        shooter: Seat,
        was_break: bool,
        has_foul: bool,
    ) -> ShotOutcome {
        if !was_break
            && !has_foul
            && self.group_of(shooter).is_none()
            && let Some(claimed_group) = summary
                .potted
                .iter()
                .find_map(|&(ball, _)| BallGroup::of_ball(ball))
        {
            self.groups = match shooter {
                Seat::One => [Some(claimed_group), Some(claimed_group.opponent())],
                Seat::Two => [Some(claimed_group.opponent()), Some(claimed_group)],
            };
        }

        for &(ball, _) in &summary.potted {
            self.record_pocketed_ball(ball, shooter);
        }

        let potted_own = if was_break {
            !summary.potted.is_empty()
        } else {
            match self.group_of(shooter) {
                None => summary
                    .potted
                    .iter()
                    .any(|&(b, _)| BallGroup::of_ball(b).is_some()),
                Some(group) => summary
                    .potted
                    .iter()
                    .any(|&(b, _)| BallGroup::of_ball(b) == Some(group)),
            }
        };

        let keeps_turn = !has_foul && potted_own;
        self.is_break_shot = false;
        self.turn_number = self.turn_number.saturating_add(1);

        if has_foul {
            self.active_seat = shooter.opponent();
            let placement = self.foul_cue_placement();
            self.cue_placement = placement;
            if summary.scratched {
                self.respawn_cue_ball(placement);
            }
        } else if keeps_turn {
            self.cue_placement = CuePlacement::Fixed;
        } else {
            self.active_seat = shooter.opponent();
            self.cue_placement = CuePlacement::Fixed;
        }

        ShotOutcome {
            simulation: sim,
            fouls,
            potted: summary.potted,
            reracked: false,
            keeps_turn,
        }
    }

    fn record_pocketed_ball(&mut self, ball: u8, shooter: Seat) {
        let [p1, p2] = &mut self.pocketed_by_seat;
        if let Some(group) = BallGroup::of_ball(ball) {
            let [g1, g2] = self.groups;
            if g1 == Some(group) {
                if !p1.contains(&ball) {
                    p1.push(ball);
                }
                return;
            }
            if g2 == Some(group) {
                if !p2.contains(&ball) {
                    p2.push(ball);
                }
                return;
            }
        }
        let target = match shooter {
            Seat::One => p1,
            Seat::Two => p2,
        };
        if !target.contains(&ball) {
            target.push(ball);
        }
    }

    fn respawn_cue_ball(&mut self, placement: CuePlacement) {
        let candidates = [
            Vec2::new(HEAD_STRING_X, CENTER_Y),
            Vec2::new(HEAD_STRING_X - 0.12, CENTER_Y),
            Vec2::new(HEAD_STRING_X - 0.12, CENTER_Y + 0.15),
            Vec2::new(HEAD_STRING_X - 0.12, CENTER_Y - 0.15),
            Vec2::new(0.35, CENTER_Y),
            Vec2::new(0.35, CENTER_Y + 0.25),
            Vec2::new(0.35, CENTER_Y - 0.25),
        ];
        let prev = self.cue_placement;
        self.cue_placement = placement;

        let mut chosen = Vec2::new(HEAD_STRING_X, CENTER_Y);
        let mut found = false;
        for pos in candidates {
            if self.is_valid_cue_placement(pos) {
                chosen = pos;
                found = true;
                break;
            }
        }
        if !found {
            'outer: for ix in 2..12 {
                for iy in 2..10 {
                    let x = f64::from(ix) * 0.05;
                    let y = f64::from(iy) * 0.11;
                    let p = Vec2::new(x, y);
                    if self.is_valid_cue_placement(p) {
                        chosen = p;
                        break 'outer;
                    }
                }
            }
        }
        self.cue_placement = prev;

        if let Some(cue) = self.balls.get_mut(0) {
            cue.pos = chosen;
            cue.vel = Vec2::ZERO;
            cue.spin_xy = Vec2::ZERO;
            cue.spin_z = 0.0;
            cue.pocketed = false;
        }
    }
}

fn summarize_events(events: &[PhysicsEvent]) -> ShotEventSummary {
    let mut first_contact: Option<(u8, u32)> = None;
    let mut scratched = false;
    let mut potted: Vec<(u8, u8)> = Vec::new();
    let mut rail_after_contact = false;
    let mut rail_balls_on_break: [bool; BALL_COUNT] = [false; BALL_COUNT];

    for ev in events {
        match *ev {
            PhysicsEvent::FirstContact { ball, time_ms } => {
                first_contact = Some((ball, time_ms));
            }
            PhysicsEvent::Scratch { .. } => scratched = true,
            PhysicsEvent::Pot { ball, pocket, .. } => potted.push((ball, pocket)),
            PhysicsEvent::CushionHit { ball, time_ms, .. } => {
                if let Some((_, contact_ms)) = first_contact
                    && time_ms >= contact_ms
                {
                    rail_after_contact = true;
                }
                if ball != 0
                    && let Some(slot) = rail_balls_on_break.get_mut(usize::from(ball))
                {
                    *slot = true;
                }
            }
            PhysicsEvent::BallHit { .. } => {}
        }
    }

    ShotEventSummary {
        first_contact: first_contact.map(|(ball, _)| ball),
        scratched,
        potted,
        rail_after_contact,
        break_rail_count: rail_balls_on_break.iter().filter(|&&hit| hit).count(),
    }
}

/// Builds a legal WPA 8-ball triangle rack:
/// - Ball 8 in the center (row 2, col 1; slot 4)
/// - One solid and one stripe in the two rear corners (slots 10 and 14)
/// - Remaining 12 balls deterministically shuffled via `rng`
#[must_use]
pub fn build_triangle_rack(rng: &mut SplitMix64) -> [BallState; BALL_COUNT] {
    let mut balls = core::array::from_fn(|id| {
        #[expect(clippy::cast_possible_truncation, reason = "id in 0..16")]
        BallState::pocketed(id as u8)
    });

    // Cue ball centered on the head string.
    balls[0] = BallState::stationary(0, Vec2::new(HEAD_STRING_X, CENTER_Y));

    let mut pool: [u8; 12] = [2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14];
    // Fisher-Yates shuffle with SplitMix64.
    for i in (1..pool.len()).rev() {
        let j_u64 = rng.next_bounded((i + 1) as u64);
        #[expect(clippy::cast_possible_truncation, reason = "j_u64 <= 12")]
        let j = j_u64 as usize;
        pool.swap(i, j);
    }

    // Reserve slot 4 = 8-ball, slot 10 = solid 1, slot 14 = stripe 15 (rear corners).
    let mut order = [0_u8; 15];
    order[4] = 8;
    order[10] = 1;
    order[14] = 15;
    let mut p_idx = 0_usize;
    for (slot, dest) in order.iter_mut().enumerate() {
        if slot == 4 || slot == 10 || slot == 14 {
            continue;
        }
        if let Some(&ball_id) = pool.get(p_idx) {
            *dest = ball_id;
        }
        p_idx += 1;
    }

    let spacing = BALL_DIAMETER + 0.0003;
    let dx = spacing * (sqrt(3.0) * 0.5);
    let dy = spacing;

    let mut idx = 0_usize;
    for row in 0_u8..5 {
        let row_f = f64::from(row);
        for col in 0..=row {
            if let Some(&ball_id) = order.get(idx)
                && let Some(slot) = balls.get_mut(usize::from(ball_id))
            {
                let col_f = f64::from(col);
                let x = FOOT_SPOT_X + row_f * dx;
                let y = CENTER_Y + (col_f - row_f * 0.5) * dy;
                *slot = BallState::stationary(ball_id, Vec2::new(x, y));
            }
            idx += 1;
        }
    }

    balls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::atan2;

    #[test]
    fn triangle_rack_places_eight_center_and_opposite_rear_corners() {
        let mut rng = SplitMix64::new(123);
        let balls = build_triangle_rack(&mut rng);
        assert!(balls.iter().all(|b| !b.pocketed));
        // Eight ball sits at row 2, center y.
        assert!((balls[8].pos.y - CENTER_Y).abs() < 1e-9);
        assert!(balls[8].pos.x > FOOT_SPOT_X + BALL_DIAMETER);
        // No two balls overlap.
        for i in 0..BALL_COUNT {
            for j in (i + 1)..BALL_COUNT {
                assert!(balls[i].pos.dist(balls[j].pos) >= BALL_DIAMETER);
            }
        }
    }

    #[test]
    fn hitting_eight_ball_first_on_open_table_is_wrong_first_ball_foul() {
        let mut game = PoolGame::new(Seat::One, PoolSettings::default(), false, 1);
        game.is_break_shot = false;
        game.cue_placement = CuePlacement::Fixed;
        for b in &mut game.balls {
            b.pocketed = true;
        }
        game.balls[0] = BallState::stationary(0, Vec2::new(0.80, CENTER_Y));
        game.balls[8] = BallState::stationary(8, Vec2::new(1.20, CENTER_Y));
        game.balls[1] = BallState::stationary(1, Vec2::new(2.00, 0.25));

        let res = game
            .shoot(
                ShotInput {
                    angle: 0.0,
                    power: 0.4,
                    spin_x: 0.0,
                    spin_y: 0.0,
                    cue_pos: None,
                    called_pocket: None,
                },
                false,
            )
            .unwrap();

        assert!(res.fouls.contains(&PoolFoul::WrongFirstBall));
        assert_eq!(game.active_seat(), Seat::Two);
        assert!(game.ball_in_hand());
    }

    #[test]
    fn potting_eight_early_loses_immediately() {
        let mut game = PoolGame::new(Seat::One, PoolSettings::default(), false, 1);
        game.is_break_shot = false;
        game.cue_placement = CuePlacement::Fixed;
        for b in &mut game.balls {
            b.pocketed = true;
        }
        game.balls[0] = BallState::stationary(0, Vec2::new(2.00, 0.54));
        game.balls[8] = BallState::stationary(8, Vec2::new(2.34, 0.20));
        game.balls[1] = BallState::stationary(1, Vec2::new(0.50, 0.50));

        let aim = atan2(0.20 - 0.54, 2.34 - 2.00);
        let _ = game
            .shoot(
                ShotInput {
                    angle: aim,
                    power: 0.45,
                    spin_x: 0.0,
                    spin_y: 0.0,
                    cue_pos: None,
                    called_pocket: None,
                },
                false,
            )
            .unwrap();

        assert_eq!(
            game.outcome(),
            Some(PoolOutcome {
                winner: Seat::Two,
                reason: PoolWinReason::OpponentEarlyEight,
            })
        );
    }
}
