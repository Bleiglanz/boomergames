//! Pure per-game Elo rating, tier badges, anti-farming decay, and High Score eligibility.

use crate::GameKind;

/// Starting Elo rating for every new player in every game.
pub const STARTING_RATING: i32 = 1200;

/// Minimum Elo floor so ratings never drift below 100.
pub const RATING_FLOOR: i32 = 100;

/// Number of rated games required to graduate from provisional (`?`) status.
pub const PROVISIONAL_GAMES_THRESHOLD: i32 = 10;

/// Maximum counted High Score wins per calendar day against the same opponent (`incubation/plan.md` H-7).
pub const MAX_HIGHSCORE_WINS_PER_OPPONENT_DAY: u32 = 5;

/// Minimum moves (Checkers plies) for a finished game to count toward High Scores.
pub const MIN_CHECKERS_PLIES_FOR_HIGHSCORE: u32 = 4;

/// Minimum shots (Pool) for a finished game to count toward High Scores.
pub const MIN_POOL_SHOTS_FOR_HIGHSCORE: u32 = 2;

/// Visual skill tier badge derived from per-game Elo rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RatingTier {
    /// Under 1100 Elo.
    Beginner,
    /// 1100–1399 Elo.
    Intermediate,
    /// 1400–1699 Elo.
    Advanced,
    /// 1700+ Elo.
    Master,
}

/// Returns the [`RatingTier`] badge for `rating`.
#[must_use]
pub const fn tier_for_rating(rating: i32) -> RatingTier {
    if rating < 1100 {
        RatingTier::Beginner
    } else if rating < 1400 {
        RatingTier::Intermediate
    } else if rating < 1700 {
        RatingTier::Advanced
    } else {
        RatingTier::Master
    }
}

/// Returns `true` when the player is still provisional (`< 10` rated games).
#[must_use]
pub const fn is_provisional(rated_games: i32) -> bool {
    rated_games < PROVISIONAL_GAMES_THRESHOLD
}

/// Match result from Seat 1's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchScore {
    /// Seat 1 won.
    SeatOneWin,
    /// Seat 2 won.
    SeatTwoWin,
    /// Match ended in a draw.
    Draw,
}

/// Input rating state for one player before a rated match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerRatingInput {
    /// Current rating before the match.
    pub rating: i32,
    /// Total rated games completed before this match.
    pub rated_games: i32,
}

/// Output rating update for one player after a rated match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerRatingUpdate {
    /// Rating before the match.
    pub old_rating: i32,
    /// Rating after the match (clamped to [`RATING_FLOOR`]).
    pub new_rating: i32,
    /// Updated rated game count (`old + 1`).
    pub new_rated_games: i32,
}

/// Anti-farming rating decay multiplier based on rated matches already played today
/// between the same pair of opponents (`0..5 => 1.0`, `5..=7 => 0.5`, `8..=10 => 0.25`, `>10 => 0.0`).
#[must_use]
pub const fn anti_farming_multiplier(pair_games_today: u32) -> f64 {
    if pair_games_today < MAX_HIGHSCORE_WINS_PER_OPPONENT_DAY {
        1.0
    } else if pair_games_today <= 7 {
        0.5
    } else if pair_games_today <= 10 {
        0.25
    } else {
        0.0
    }
}

/// Returns `true` if a finished human-vs-human match qualifies to increment the High Score table.
#[must_use]
pub const fn qualifies_for_highscore(
    game: GameKind,
    is_human_vs_human: bool,
    has_winner: bool,
    activity_count: u32,
    pair_counted_wins_today: u32,
) -> bool {
    if !is_human_vs_human || !has_winner {
        return false;
    }
    if pair_counted_wins_today >= MAX_HIGHSCORE_WINS_PER_OPPONENT_DAY {
        return false;
    }
    let min_activity = match game {
        GameKind::Checkers => MIN_CHECKERS_PLIES_FOR_HIGHSCORE,
        GameKind::Pool => MIN_POOL_SHOTS_FOR_HIGHSCORE,
    };
    activity_count >= min_activity
}

/// Computes deterministic post-match Elo ratings for both seats using `libm` math.
#[must_use]
pub fn calculate_match_elo(
    seat_one: PlayerRatingInput,
    seat_two: PlayerRatingInput,
    score: MatchScore,
    pair_games_today: u32,
) -> (PlayerRatingUpdate, PlayerRatingUpdate) {
    let decay = anti_farming_multiplier(pair_games_today);

    let (actual_one, actual_two) = match score {
        MatchScore::SeatOneWin => (1.0, 0.0),
        MatchScore::SeatTwoWin => (0.0, 1.0),
        MatchScore::Draw => (0.5, 0.5),
    };

    let expected_one = expected_score(seat_one.rating, seat_two.rating);
    let expected_two = 1.0 - expected_one;

    let delta_one = rounded_delta(k_factor(seat_one) * decay * (actual_one - expected_one));
    let delta_two = rounded_delta(k_factor(seat_two) * decay * (actual_two - expected_two));

    let out_one = PlayerRatingUpdate {
        old_rating: seat_one.rating,
        new_rating: (seat_one.rating + delta_one).max(RATING_FLOOR),
        new_rated_games: seat_one.rated_games.saturating_add(1),
    };
    let out_two = PlayerRatingUpdate {
        old_rating: seat_two.rating,
        new_rating: (seat_two.rating + delta_two).max(RATING_FLOOR),
        new_rated_games: seat_two.rated_games.saturating_add(1),
    };
    (out_one, out_two)
}

fn expected_score(rating_a: i32, rating_b: i32) -> f64 {
    let diff = f64::from(rating_b - rating_a) / 400.0;
    1.0 / (1.0 + libm::pow(10.0, diff))
}

const fn k_factor(input: PlayerRatingInput) -> f64 {
    if is_provisional(input.rated_games) {
        32.0
    } else if input.rating < 1800 {
        24.0
    } else {
        16.0
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Elo delta is bounded within [-32, +32]"
)]
fn rounded_delta(raw: f64) -> i32 {
    libm::round(raw) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_provisional_players_exchange_sixteen_points() {
        let p = PlayerRatingInput {
            rating: STARTING_RATING,
            rated_games: 0,
        };
        let (u1, u2) = calculate_match_elo(p, p, MatchScore::SeatOneWin, 0);
        assert_eq!(u1.new_rating, 1216);
        assert_eq!(u2.new_rating, 1184);
        assert_eq!(u1.new_rated_games, 1);
        assert!(is_provisional(u1.new_rated_games));
    }

    #[test]
    fn anti_farming_decay_reduces_and_caps_repeated_pair_games() {
        let p = PlayerRatingInput {
            rating: 1400,
            rated_games: 20,
        };
        let (full, _) = calculate_match_elo(p, p, MatchScore::SeatOneWin, 0);
        let (half, _) = calculate_match_elo(p, p, MatchScore::SeatOneWin, 5);
        let (quarter, _) = calculate_match_elo(p, p, MatchScore::SeatOneWin, 8);
        let (zero, _) = calculate_match_elo(p, p, MatchScore::SeatOneWin, 11);
        assert_eq!(full.new_rating - full.old_rating, 12);
        assert_eq!(half.new_rating - half.old_rating, 6);
        assert_eq!(quarter.new_rating - quarter.old_rating, 3);
        assert_eq!(zero.new_rating - zero.old_rating, 0);
    }

    #[test]
    fn highscore_eligibility_enforces_human_activity_and_daily_cap() {
        assert!(qualifies_for_highscore(
            GameKind::Checkers,
            true,
            true,
            4,
            0
        ));
        assert!(!qualifies_for_highscore(
            GameKind::Checkers,
            true,
            true,
            3,
            0
        ));
        assert!(!qualifies_for_highscore(
            GameKind::Checkers,
            false,
            true,
            10,
            0
        ));
        assert!(!qualifies_for_highscore(GameKind::Pool, true, true, 5, 5));
        assert!(qualifies_for_highscore(GameKind::Pool, true, true, 2, 4));
    }

    #[test]
    fn tier_boundaries_match_spec() {
        assert_eq!(tier_for_rating(1050), RatingTier::Beginner);
        assert_eq!(tier_for_rating(1200), RatingTier::Intermediate);
        assert_eq!(tier_for_rating(1500), RatingTier::Advanced);
        assert_eq!(tier_for_rating(1750), RatingTier::Master);
    }
}
