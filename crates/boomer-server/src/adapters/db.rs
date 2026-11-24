//! PostgreSQL persistence adapter powered by `sqlx` using `migrations/0001_initial.sql`.

use boomer_domain::GameKind;
use boomer_domain::rating::{
    MatchScore, PlayerRatingInput, STARTING_RATING, calculate_match_elo, qualifies_for_highscore,
};
use chrono::{DateTime, Duration, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

use crate::auth::{MAX_AVATAR_INDEX, NAME_CHANGE_COOLDOWN_DAYS, validate_display_name};
use crate::mapping::{game_kind_from_db, game_kind_to_db};
use crate::ports::{
    BoxFuture, HealthCheck, HighscoreEntry, MatchHistoryItem, MatchRepo, PlayerStatRecord,
    PortError, RecordMatchInput, RecordMatchOutcome, RecordedRatingChange, RecordedWinCount,
    ReportRepo, StatsRepo, UserRecord, UserRepo,
};

/// PostgreSQL adapter implementing all server persistence ports.
#[derive(Debug, Clone)]
pub struct PgRepos {
    pool: PgPool,
}

impl PgRepos {
    /// Connects to PostgreSQL and executes embedded `migrations/` automatically on startup.
    ///
    /// # Errors
    /// Returns [`PortError::Unavailable`] if connection or migration execution fails.
    pub async fn connect_and_migrate(database_url: &str) -> Result<Self, PortError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .connect(database_url)
            .await
            .map_err(|err| PortError::Unavailable(format!("postgres connect failed: {err}")))?;

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .map_err(|err| PortError::Unavailable(format!("postgres migrate failed: {err}")))?;

        Ok(Self { pool })
    }

    fn row_to_user(row: &sqlx::postgres::PgRow) -> Result<UserRecord, PortError> {
        let map_err = |e: sqlx::Error| PortError::Unavailable(e.to_string());
        Ok(UserRecord {
            id: row.try_get("id").map_err(map_err)?,
            email: row.try_get("email").map_err(map_err)?,
            google_sub: row.try_get("google_sub").map_err(map_err)?,
            display_name: row.try_get("display_name").map_err(map_err)?,
            name_changed_at: row.try_get("name_changed_at").map_err(map_err)?,
            avatar: row.try_get("avatar").map_err(map_err)?,
            ignored_ids: row.try_get("ignored_ids").map_err(map_err)?,
            is_moderator: row.try_get("is_moderator").map_err(map_err)?,
            muted_until: row.try_get("muted_until").map_err(map_err)?,
            banned_until: row.try_get("banned_until").map_err(map_err)?,
            session_version: row.try_get("session_version").map_err(map_err)?,
            created_at: row.try_get("created_at").map_err(map_err)?,
        })
    }
}

impl UserRepo for PgRepos {
    fn find_by_id(&self, id: i64) -> BoxFuture<'_, Result<Option<UserRecord>, PortError>> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id, email::text, google_sub, display_name::text, name_changed_at, \
                 avatar, ignored_ids, is_moderator, muted_until, banned_until, session_version, created_at \
                 FROM users WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?;

            row.as_ref().map(Self::row_to_user).transpose()
        })
    }

    fn find_or_create_by_email(
        &self,
        email: &str,
        google_sub: Option<&str>,
        suggested_name: Option<&str>,
    ) -> BoxFuture<'_, Result<(UserRecord, bool), PortError>> {
        let email_norm = email.trim().to_ascii_lowercase();
        let google_sub = google_sub.map(str::to_owned);
        let suggested = suggested_name.map(str::to_owned);

        Box::pin(async move {
            if let Some(row) = sqlx::query(
                "SELECT id, email::text, google_sub, display_name::text, name_changed_at, \
                 avatar, ignored_ids, is_moderator, muted_until, banned_until, session_version, created_at \
                 FROM users WHERE email = $1",
            )
            .bind(&email_norm)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?
            {
                let mut user = Self::row_to_user(&row)?;
                if user.google_sub.is_none()
                    && let Some(sub) = google_sub
                {
                    let _ = sqlx::query("UPDATE users SET google_sub = $1 WHERE id = $2")
                        .bind(&sub)
                        .bind(user.id)
                        .execute(&self.pool)
                        .await;
                    user.google_sub = Some(sub);
                }
                return Ok((user, false));
            }

            let suffix = Utc::now().timestamp_subsec_micros();
            let initial_name = suggested
                .as_deref()
                .and_then(|s| validate_display_name(s).ok())
                .unwrap_or_else(|| format!("Player_{suffix}"));

            let row = sqlx::query(
                "INSERT INTO users (email, google_sub, display_name) VALUES ($1, $2, $3) \
                 RETURNING id, email::text, google_sub, display_name::text, name_changed_at, \
                 avatar, ignored_ids, is_moderator, muted_until, banned_until, session_version, created_at",
            )
            .bind(&email_norm)
            .bind(&google_sub)
            .bind(&initial_name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?;

            Ok((Self::row_to_user(&row)?, true))
        })
    }

    fn update_account(
        &self,
        user_id: i64,
        new_name: Option<&str>,
        new_avatar: Option<i16>,
        now: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<UserRecord, PortError>> {
        let new_name = new_name.map(str::to_owned);
        Box::pin(async move {
            if let Some(avatar) = new_avatar
                && !(0..=MAX_AVATAR_INDEX).contains(&avatar)
            {
                return Err(PortError::Invalid("avatar out of range".to_owned()));
            }

            let existing = self.find_by_id(user_id).await?.ok_or(PortError::NotFound)?;

            let (final_name, final_changed_at) = if let Some(ref candidate) = new_name {
                let valid = validate_display_name(candidate)
                    .map_err(|msg| PortError::Invalid(msg.to_owned()))?;
                if existing.display_name.eq_ignore_ascii_case(&valid) {
                    (existing.display_name, existing.name_changed_at)
                } else {
                    if let Some(last_change) = existing.name_changed_at
                        && now < last_change + Duration::days(NAME_CHANGE_COOLDOWN_DAYS)
                    {
                        return Err(PortError::Conflict(
                            "display name may only be changed once every 30 days".to_owned(),
                        ));
                    }
                    (valid, Some(now))
                }
            } else {
                (existing.display_name, existing.name_changed_at)
            };

            let final_avatar = new_avatar.unwrap_or(existing.avatar);

            let row = sqlx::query(
                "UPDATE users SET display_name = $1, name_changed_at = $2, avatar = $3 \
                 WHERE id = $4 \
                 RETURNING id, email::text, google_sub, display_name::text, name_changed_at, \
                 avatar, ignored_ids, is_moderator, muted_until, banned_until, session_version, created_at",
            )
            .bind(&final_name)
            .bind(final_changed_at)
            .bind(final_avatar)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("unique") || msg.contains("23505") {
                    PortError::Conflict("display name taken".to_owned())
                } else {
                    PortError::Unavailable(msg)
                }
            })?;

            Self::row_to_user(&row)
        })
    }

    fn delete_user(&self, user_id: i64) -> BoxFuture<'_, Result<(), PortError>> {
        Box::pin(async move {
            let res = sqlx::query("DELETE FROM users WHERE id = $1")
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(|e| PortError::Unavailable(e.to_string()))?;
            if res.rows_affected() == 0 {
                return Err(PortError::NotFound);
            }
            Ok(())
        })
    }

    fn set_ignored(
        &self,
        user_id: i64,
        target_id: i64,
        ignored: bool,
    ) -> BoxFuture<'_, Result<Vec<i64>, PortError>> {
        Box::pin(async move {
            let sql = if ignored {
                "UPDATE users \
                 SET ignored_ids = CASE \
                     WHEN $2 = ANY(ignored_ids) THEN ignored_ids \
                     ELSE array_append(ignored_ids, $2) \
                 END \
                 WHERE id = $1 RETURNING ignored_ids"
            } else {
                "UPDATE users \
                 SET ignored_ids = array_remove(ignored_ids, $2) \
                 WHERE id = $1 RETURNING ignored_ids"
            };
            let row = sqlx::query(sql)
                .bind(user_id)
                .bind(target_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| PortError::Unavailable(e.to_string()))?
                .ok_or(PortError::NotFound)?;
            row.try_get("ignored_ids")
                .map_err(|e| PortError::Unavailable(e.to_string()))
        })
    }
}

impl StatsRepo for PgRepos {
    fn stats_for_user(
        &self,
        user_id: i64,
    ) -> BoxFuture<'_, Result<Vec<PlayerStatRecord>, PortError>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT user_id, game, rating, rated_games, games, wins, wins_reached_at \
                 FROM player_stats WHERE user_id = $1",
            )
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?;

            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let game_str: String = row
                    .try_get("game")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                if let Some(game) = game_kind_from_db(&game_str) {
                    out.push(PlayerStatRecord {
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| PortError::Unavailable(e.to_string()))?,
                        game,
                        rating: row
                            .try_get("rating")
                            .map_err(|e| PortError::Unavailable(e.to_string()))?,
                        rated_games: row
                            .try_get("rated_games")
                            .map_err(|e| PortError::Unavailable(e.to_string()))?,
                        games: row
                            .try_get("games")
                            .map_err(|e| PortError::Unavailable(e.to_string()))?,
                        wins: row
                            .try_get("wins")
                            .map_err(|e| PortError::Unavailable(e.to_string()))?,
                        wins_reached_at: row
                            .try_get("wins_reached_at")
                            .map_err(|e| PortError::Unavailable(e.to_string()))?,
                    });
                }
            }
            Ok(out)
        })
    }

    fn highscores(
        &self,
        game: GameKind,
        around_user_id: Option<i64>,
    ) -> BoxFuture<'_, Result<Vec<HighscoreEntry>, PortError>> {
        let game_str = game_kind_to_db(game);
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT ROW_NUMBER() OVER (ORDER BY ps.wins DESC, ps.wins_reached_at ASC NULLS LAST, ps.user_id ASC) AS rank, \
                 u.id, u.display_name::text, u.avatar, ps.wins, ps.games \
                 FROM player_stats ps \
                 JOIN users u ON u.id = ps.user_id \
                 WHERE ps.game = $1 AND ps.wins > 0 AND (u.banned_until IS NULL OR u.banned_until <= now()) \
                 ORDER BY ps.wins DESC, ps.wins_reached_at ASC NULLS LAST, ps.user_id ASC \
                 LIMIT 500",
            )
            .bind(game_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?;

            let mut entries = Vec::with_capacity(rows.len());
            for row in rows {
                let rank_i64: i64 = row
                    .try_get("rank")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let wins_i32: i32 = row
                    .try_get("wins")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let games_i32: i32 = row
                    .try_get("games")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                entries.push(HighscoreEntry {
                    rank: u32::try_from(rank_i64).unwrap_or(u32::MAX),
                    user_id: row
                        .try_get("id")
                        .map_err(|e| PortError::Unavailable(e.to_string()))?,
                    display_name: row
                        .try_get("display_name")
                        .map_err(|e| PortError::Unavailable(e.to_string()))?,
                    avatar: row
                        .try_get("avatar")
                        .map_err(|e| PortError::Unavailable(e.to_string()))?,
                    wins: u32::try_from(wins_i32).unwrap_or(0),
                    games: u32::try_from(games_i32).unwrap_or(0),
                });
            }

            if let Some(target_uid) = around_user_id
                && let Some(pos) = entries.iter().position(|row| row.user_id == target_uid)
            {
                let start = pos.saturating_sub(10);
                let end = (pos + 11).min(entries.len());
                return Ok(entries.get(start..end).unwrap_or(&[]).to_vec());
            }

            entries.truncate(100);
            Ok(entries)
        })
    }
}

impl MatchRepo for PgRepos {
    fn recent_matches(
        &self,
        user_id: i64,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<MatchHistoryItem>, PortError>> {
        let limit_i64 = i64::try_from(limit).unwrap_or(20);
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT m.game, m.player1_id, m.player2_id, m.bot_level, m.winner, m.ended_at, \
                 u1.display_name::text AS p1_name, u2.display_name::text AS p2_name \
                 FROM matches m \
                 LEFT JOIN users u1 ON u1.id = m.player1_id \
                 LEFT JOIN users u2 ON u2.id = m.player2_id \
                 WHERE m.player1_id = $1 OR m.player2_id = $1 \
                 ORDER BY m.ended_at DESC \
                 LIMIT $2",
            )
            .bind(user_id)
            .bind(limit_i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?;

            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let game_str: String = row
                    .try_get("game")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let Some(game) = game_kind_from_db(&game_str) else {
                    continue;
                };
                let p1_id: Option<i64> = row
                    .try_get("player1_id")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let bot_level: Option<i16> = row
                    .try_get("bot_level")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let winner: Option<i16> = row
                    .try_get("winner")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let p1_name: Option<String> = row
                    .try_get("p1_name")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let p2_name: Option<String> = row
                    .try_get("p2_name")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;
                let ended_at: DateTime<Utc> = row
                    .try_get("ended_at")
                    .map_err(|e| PortError::Unavailable(e.to_string()))?;

                let is_p1 = p1_id == Some(user_id);
                let opponent_name = if let Some(lvl) = bot_level {
                    format!("House Bot (Level {lvl})")
                } else if is_p1 {
                    p2_name.unwrap_or_else(|| "Guest".to_owned())
                } else {
                    p1_name.unwrap_or_else(|| "Guest".to_owned())
                };
                let result = match winner {
                    None => "draw",
                    Some(1) if is_p1 => "win",
                    Some(2) if !is_p1 => "win",
                    _ => "loss",
                };
                out.push(MatchHistoryItem {
                    game,
                    opponent_name,
                    result: result.to_owned(),
                    ended_at,
                });
            }
            Ok(out)
        })
    }

    fn record_finished_match(
        &self,
        input: RecordMatchInput,
    ) -> BoxFuture<'_, Result<RecordMatchOutcome, PortError>> {
        Box::pin(async move { pg_record_finished_match(&self.pool, input).await })
    }
}

async fn pg_record_finished_match(
    pool: &PgPool,
    input: RecordMatchInput,
) -> Result<RecordMatchOutcome, PortError> {
    let map_err = |e: sqlx::Error| PortError::Unavailable(e.to_string());
    let game_str = game_kind_to_db(input.game);
    let is_human_vs_human =
        input.bot_level.is_none() && input.player1_id.is_some() && input.player2_id.is_some();
    let rated = input.rated_requested && is_human_vs_human;

    let mut tx = pool.begin().await.map_err(map_err)?;

    let (pair_rated_today, pair_wins_today) = if let (Some(first_uid), Some(second_uid)) =
        (input.player1_id, input.player2_id)
    {
        let row = sqlx::query(
                "SELECT \
                 COUNT(*) FILTER (WHERE rated) AS rated_cnt, \
                 COUNT(*) FILTER (WHERE counts_for_highscore) AS win_cnt \
                 FROM matches \
                 WHERE game = $1 AND ended_at >= date_trunc('day', now()) \
                   AND ((player1_id = $2 AND player2_id = $3) OR (player1_id = $3 AND player2_id = $2))",
            )
            .bind(game_str)
            .bind(first_uid)
            .bind(second_uid)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_err)?;
        let r_cnt: i64 = row.try_get("rated_cnt").map_err(map_err)?;
        let w_cnt: i64 = row.try_get("win_cnt").map_err(map_err)?;
        (
            u32::try_from(r_cnt).unwrap_or(u32::MAX),
            u32::try_from(w_cnt).unwrap_or(u32::MAX),
        )
    } else {
        (0, 0)
    };

    let counts_for_highscore = qualifies_for_highscore(
        input.game,
        is_human_vs_human,
        input.winner.is_some(),
        input.activity_count,
        pair_wins_today,
    );

    let _ = sqlx::query(
        "INSERT INTO matches (game, player1_id, player2_id, bot_level, winner, reason, rated, counts_for_highscore) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(game_str)
    .bind(input.player1_id)
    .bind(input.player2_id)
    .bind(input.bot_level)
    .bind(input.winner)
    .bind(&input.reason)
    .bind(rated)
    .bind(counts_for_highscore)
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;

    let mut outcome = RecordMatchOutcome {
        rated,
        counts_for_highscore,
        rating_changes: Vec::new(),
        win_counts: Vec::new(),
    };

    if rated && let (Some(first_uid), Some(second_uid)) = (input.player1_id, input.player2_id) {
        pg_apply_rated_updates(
            &mut tx,
            game_str,
            first_uid,
            second_uid,
            input.winner,
            pair_rated_today,
            counts_for_highscore,
            &mut outcome,
        )
        .await?;
    } else if input.bot_level.is_none() {
        for (uid_opt, seat_no) in [(input.player1_id, 1_i16), (input.player2_id, 2_i16)] {
            if let Some(uid) = uid_opt
                && let Some(wins) = pg_upsert_stat(
                    &mut tx,
                    uid,
                    game_str,
                    None,
                    counts_for_highscore && input.winner == Some(seat_no),
                )
                .await?
            {
                outcome
                    .win_counts
                    .push(RecordedWinCount { user_id: uid, wins });
            }
        }
    }

    tx.commit().await.map_err(map_err)?;
    Ok(outcome)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Transact helper passes explicit SQL transaction and match attributes"
)]
async fn pg_apply_rated_updates(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_str: &str,
    first_uid: i64,
    second_uid: i64,
    winner: Option<i16>,
    pair_rated_today: u32,
    counts_for_highscore: bool,
    outcome: &mut RecordMatchOutcome,
) -> Result<(), PortError> {
    let first_in = pg_fetch_rating(tx, first_uid, game_str).await?;
    let second_in = pg_fetch_rating(tx, second_uid, game_str).await?;
    let score = match winner {
        Some(1) => MatchScore::SeatOneWin,
        Some(2) => MatchScore::SeatTwoWin,
        _ => MatchScore::Draw,
    };
    let (upd_one, upd_two) = calculate_match_elo(first_in, second_in, score, pair_rated_today);
    for (uid, upd, won_seat) in [(first_uid, upd_one, 1_i16), (second_uid, upd_two, 2_i16)] {
        if let Some(wins) = pg_upsert_stat(
            tx,
            uid,
            game_str,
            Some((upd.new_rating, upd.new_rated_games)),
            counts_for_highscore && winner == Some(won_seat),
        )
        .await?
        {
            outcome
                .win_counts
                .push(RecordedWinCount { user_id: uid, wins });
        }
        outcome.rating_changes.push(RecordedRatingChange {
            user_id: uid,
            old_rating: upd.old_rating,
            new_rating: upd.new_rating,
            new_rated_games: upd.new_rated_games,
        });
    }
    Ok(())
}

async fn pg_fetch_rating(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    game_str: &str,
) -> Result<PlayerRatingInput, PortError> {
    let map_err = |e: sqlx::Error| PortError::Unavailable(e.to_string());
    let row = sqlx::query(
        "SELECT rating, rated_games FROM player_stats WHERE user_id = $1 AND game = $2",
    )
    .bind(user_id)
    .bind(game_str)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_err)?;
    match row {
        Some(r) => Ok(PlayerRatingInput {
            rating: r.try_get("rating").map_err(map_err)?,
            rated_games: r.try_get("rated_games").map_err(map_err)?,
        }),
        None => Ok(PlayerRatingInput {
            rating: STARTING_RATING,
            rated_games: 0,
        }),
    }
}

async fn pg_upsert_stat(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    game_str: &str,
    new_rating: Option<(i32, i32)>,
    counted_win: bool,
) -> Result<Option<u32>, PortError> {
    let map_err = |e: sqlx::Error| PortError::Unavailable(e.to_string());
    let (rating_val, rated_games_val, has_rating) = match new_rating {
        Some((r, rg)) => (r, rg, true),
        None => (STARTING_RATING, 0, false),
    };
    let win_inc: i32 = i32::from(counted_win);
    let row = sqlx::query(
        "INSERT INTO player_stats (user_id, game, rating, rated_games, games, wins, wins_reached_at) \
         VALUES ($1, $2, $3, $4, 1, $5, CASE WHEN $5 > 0 THEN now() ELSE NULL END) \
         ON CONFLICT (user_id, game) DO UPDATE SET \
           rating = CASE WHEN $6 THEN $3 ELSE player_stats.rating END, \
           rated_games = CASE WHEN $6 THEN $4 ELSE player_stats.rated_games END, \
           games = player_stats.games + 1, \
           wins = player_stats.wins + $5, \
           wins_reached_at = CASE WHEN $5 > 0 THEN now() ELSE player_stats.wins_reached_at END \
         RETURNING wins",
    )
    .bind(user_id)
    .bind(game_str)
    .bind(rating_val)
    .bind(rated_games_val)
    .bind(win_inc)
    .bind(has_rating)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_err)?;
    if counted_win {
        let wins_i32: i32 = row.try_get("wins").map_err(map_err)?;
        Ok(Some(u32::try_from(wins_i32).unwrap_or(0)))
    } else {
        Ok(None)
    }
}

impl ReportRepo for PgRepos {
    fn create_report(
        &self,
        reporter_id: Option<i64>,
        target_id: Option<i64>,
        target_name: &str,
        reason: &str,
        chat_log: Option<&str>,
    ) -> BoxFuture<'_, Result<i64, PortError>> {
        let target_name = target_name.to_owned();
        let reason = reason.to_owned();
        let chat_log = chat_log.map(str::to_owned);
        Box::pin(async move {
            let row = sqlx::query(
                "INSERT INTO reports (reporter_id, reported_id, reported_name, reason, chat_log) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(reporter_id)
            .bind(target_id)
            .bind(target_name)
            .bind(reason)
            .bind(chat_log)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| PortError::Unavailable(e.to_string()))?;

            row.try_get("id")
                .map_err(|e| PortError::Unavailable(e.to_string()))
        })
    }
}

impl HealthCheck for PgRepos {
    fn ping(&self) -> BoxFuture<'_, Result<(), PortError>> {
        Box::pin(async move {
            let _ = sqlx::query("SELECT 1")
                .execute(&self.pool)
                .await
                .map_err(|e| PortError::Unavailable(e.to_string()))?;
            Ok(())
        })
    }
}
