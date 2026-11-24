//! PostgreSQL persistence adapter powered by `sqlx` using `migrations/0001_initial.sql`.

use boomer_domain::GameKind;
use chrono::{DateTime, Duration, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

use crate::auth::{MAX_AVATAR_INDEX, NAME_CHANGE_COOLDOWN_DAYS, validate_display_name};
use crate::mapping::{game_kind_from_db, game_kind_to_db};
use crate::ports::{
    BoxFuture, HealthCheck, HighscoreEntry, MatchHistoryItem, MatchRepo, PlayerStatRecord,
    PortError, ReportRepo, StatsRepo, UserRecord, UserRepo,
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
