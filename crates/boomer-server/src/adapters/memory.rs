//! Thread-safe in-memory adapters for zero-dependency local dev and automated integration tests.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use boomer_domain::GameKind;
use chrono::{DateTime, Duration, Utc};

use crate::auth::{MAX_AVATAR_INDEX, NAME_CHANGE_COOLDOWN_DAYS, validate_display_name};
use crate::ports::{
    BoxFuture, GoogleTokenVerifier, HealthCheck, HighscoreEntry, MailSender, MatchHistoryItem,
    MatchRepo, PlayerStatRecord, PortError, ReportRepo, StatsRepo, UserRecord, UserRepo,
    VerifiedGoogleUser,
};

#[derive(Debug, Default)]
struct MemData {
    users: BTreeMap<i64, UserRecord>,
    stats: BTreeMap<(i64, u8), PlayerStatRecord>,
    matches: Vec<StoredMatch>,
    reports: Vec<(i64, String, String)>,
}

#[derive(Debug, Clone)]
struct StoredMatch {
    game: GameKind,
    player1_id: Option<i64>,
    player2_id: Option<i64>,
    opponent_label: String,
    winner: Option<i16>,
    ended_at: DateTime<Utc>,
}

const fn game_key(game: GameKind) -> u8 {
    match game {
        GameKind::Checkers => 0,
        GameKind::Pool => 1,
    }
}

/// In-memory persistence implementation of all repository ports.
#[derive(Debug)]
pub struct MemoryRepos {
    data: Mutex<MemData>,
    next_user_id: AtomicI64,
    next_report_id: AtomicI64,
    healthy: AtomicBool,
}

impl Default for MemoryRepos {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryRepos {
    /// Creates an empty, healthy in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Mutex::new(MemData::default()),
            next_user_id: AtomicI64::new(1),
            next_report_id: AtomicI64::new(1),
            healthy: AtomicBool::new(true),
        }
    }

    /// Toggles simulated database health (useful for testing `GET /status` 503 degradation).
    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::Relaxed);
    }

    /// Seeds a player stat row (useful for unit/integration tests).
    ///
    /// # Errors
    /// Returns [`PortError::Unavailable`] if the internal lock is poisoned.
    pub fn upsert_stat(&self, stat: PlayerStatRecord) -> Result<(), PortError> {
        let mut guard = self
            .data
            .lock()
            .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
        guard
            .stats
            .insert((stat.user_id, game_key(stat.game)), stat);
        Ok(())
    }

    /// Seeds a finished match row for testing profile history.
    ///
    /// # Errors
    /// Returns [`PortError::Unavailable`] if the internal lock is poisoned.
    pub fn insert_match(
        &self,
        game: GameKind,
        player1_id: Option<i64>,
        player2_id: Option<i64>,
        opponent_label: String,
        winner: Option<i16>,
    ) -> Result<(), PortError> {
        let mut guard = self
            .data
            .lock()
            .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
        guard.matches.push(StoredMatch {
            game,
            player1_id,
            player2_id,
            opponent_label,
            winner,
            ended_at: Utc::now(),
        });
        Ok(())
    }
}

impl UserRepo for MemoryRepos {
    fn find_by_id(&self, id: i64) -> BoxFuture<'_, Result<Option<UserRecord>, PortError>> {
        Box::pin(async move {
            let guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            Ok(guard.users.get(&id).cloned())
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
            let mut guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;

            if let Some(existing) = guard
                .users
                .values_mut()
                .find(|u| u.email.eq_ignore_ascii_case(&email_norm))
            {
                if existing.google_sub.is_none() && google_sub.is_some() {
                    existing.google_sub = google_sub;
                }
                return Ok((existing.clone(), false));
            }

            let id = self.next_user_id.fetch_add(1, Ordering::Relaxed);
            let base_name = suggested
                .as_deref()
                .and_then(|s| validate_display_name(s).ok())
                .filter(|name| {
                    !guard
                        .users
                        .values()
                        .any(|u| u.display_name.eq_ignore_ascii_case(name))
                })
                .unwrap_or_else(|| format!("Player_{id}"));

            let user = UserRecord {
                id,
                email: email_norm,
                google_sub,
                display_name: base_name,
                name_changed_at: None,
                avatar: 0,
                ignored_ids: Vec::new(),
                is_moderator: false,
                muted_until: None,
                banned_until: None,
                session_version: 0,
                created_at: Utc::now(),
            };
            guard.users.insert(id, user.clone());
            Ok((user, true))
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
            let mut guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;

            if let Some(avatar) = new_avatar
                && !(0..=MAX_AVATAR_INDEX).contains(&avatar)
            {
                return Err(PortError::Invalid("avatar out of range".to_owned()));
            }

            if let Some(ref candidate) = new_name {
                let valid = validate_display_name(candidate)
                    .map_err(|msg| PortError::Invalid(msg.to_owned()))?;
                let taken = guard
                    .users
                    .values()
                    .any(|u| u.id != user_id && u.display_name.eq_ignore_ascii_case(&valid));
                if taken {
                    return Err(PortError::Conflict("display name taken".to_owned()));
                }
                let user = guard.users.get_mut(&user_id).ok_or(PortError::NotFound)?;
                if !user.display_name.eq_ignore_ascii_case(&valid) {
                    if let Some(last_change) = user.name_changed_at
                        && now < last_change + Duration::days(NAME_CHANGE_COOLDOWN_DAYS)
                    {
                        return Err(PortError::Conflict(
                            "display name may only be changed once every 30 days".to_owned(),
                        ));
                    }
                    user.display_name = valid;
                    user.name_changed_at = Some(now);
                }
            }

            let user = guard.users.get_mut(&user_id).ok_or(PortError::NotFound)?;
            if let Some(avatar) = new_avatar {
                user.avatar = avatar;
            }
            Ok(user.clone())
        })
    }

    fn delete_user(&self, user_id: i64) -> BoxFuture<'_, Result<(), PortError>> {
        Box::pin(async move {
            let mut guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            if guard.users.remove(&user_id).is_none() {
                return Err(PortError::NotFound);
            }
            guard.stats.retain(|&(uid, _), _| uid != user_id);
            for m in &mut guard.matches {
                if m.player1_id == Some(user_id) {
                    m.player1_id = None;
                }
                if m.player2_id == Some(user_id) {
                    m.player2_id = None;
                }
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
            let mut guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            let user = guard.users.get_mut(&user_id).ok_or(PortError::NotFound)?;
            if ignored {
                if !user.ignored_ids.contains(&target_id) {
                    user.ignored_ids.push(target_id);
                }
            } else {
                user.ignored_ids.retain(|&id| id != target_id);
            }
            Ok(user.ignored_ids.clone())
        })
    }
}

impl StatsRepo for MemoryRepos {
    fn stats_for_user(
        &self,
        user_id: i64,
    ) -> BoxFuture<'_, Result<Vec<PlayerStatRecord>, PortError>> {
        Box::pin(async move {
            let guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            let rows = guard
                .stats
                .iter()
                .filter(|&(&(uid, _), _)| uid == user_id)
                .map(|(_, rec)| rec.clone())
                .collect();
            Ok(rows)
        })
    }

    fn highscores(
        &self,
        game: GameKind,
        around_user_id: Option<i64>,
    ) -> BoxFuture<'_, Result<Vec<HighscoreEntry>, PortError>> {
        Box::pin(async move {
            let guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            let now = Utc::now();
            let mut candidates: Vec<(&PlayerStatRecord, &UserRecord)> = guard
                .stats
                .values()
                .filter(|s| s.game == game && s.wins > 0)
                .filter_map(|s| {
                    let user = guard.users.get(&s.user_id)?;
                    // Banned accounts are hidden from the high score table (H-8).
                    if user.banned_until.is_some_and(|until| until > now) {
                        return None;
                    }
                    Some((s, user))
                })
                .collect();

            candidates.sort_by(|(a, _), (b, _)| {
                b.wins
                    .cmp(&a.wins)
                    .then_with(|| a.wins_reached_at.cmp(&b.wins_reached_at))
                    .then_with(|| a.user_id.cmp(&b.user_id))
            });

            let ranked: Vec<HighscoreEntry> = candidates
                .into_iter()
                .enumerate()
                .map(|(idx, (stat, user))| HighscoreEntry {
                    rank: u32::try_from(idx + 1).unwrap_or(u32::MAX),
                    user_id: user.id,
                    display_name: user.display_name.clone(),
                    avatar: user.avatar,
                    wins: u32::try_from(stat.wins).unwrap_or(0),
                    games: u32::try_from(stat.games).unwrap_or(0),
                })
                .collect();

            if let Some(target_uid) = around_user_id
                && let Some(pos) = ranked.iter().position(|row| row.user_id == target_uid)
            {
                let start = pos.saturating_sub(10);
                let end = (pos + 11).min(ranked.len());
                return Ok(ranked.get(start..end).unwrap_or(&[]).to_vec());
            }

            Ok(ranked.into_iter().take(100).collect())
        })
    }
}

impl MatchRepo for MemoryRepos {
    fn recent_matches(
        &self,
        user_id: i64,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<MatchHistoryItem>, PortError>> {
        Box::pin(async move {
            let guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            let mut rows: Vec<MatchHistoryItem> = guard
                .matches
                .iter()
                .rev()
                .filter(|m| m.player1_id == Some(user_id) || m.player2_id == Some(user_id))
                .take(limit)
                .map(|m| {
                    let is_p1 = m.player1_id == Some(user_id);
                    let result = match m.winner {
                        None => "draw",
                        Some(1) if is_p1 => "win",
                        Some(2) if !is_p1 => "win",
                        _ => "loss",
                    };
                    MatchHistoryItem {
                        game: m.game,
                        opponent_name: m.opponent_label.clone(),
                        result: result.to_owned(),
                        ended_at: m.ended_at,
                    }
                })
                .collect();
            rows.truncate(limit);
            Ok(rows)
        })
    }
}

impl ReportRepo for MemoryRepos {
    fn create_report(
        &self,
        _reporter_id: Option<i64>,
        _reported_id: Option<i64>,
        reported_name: &str,
        reason: &str,
        _chat_log: Option<&str>,
    ) -> BoxFuture<'_, Result<i64, PortError>> {
        let reported_name = reported_name.to_owned();
        let reason = reason.to_owned();
        Box::pin(async move {
            let id = self.next_report_id.fetch_add(1, Ordering::Relaxed);
            let mut guard = self
                .data
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            guard.reports.push((id, reported_name, reason));
            Ok(id)
        })
    }
}

impl HealthCheck for MemoryRepos {
    fn ping(&self) -> BoxFuture<'_, Result<(), PortError>> {
        Box::pin(async move {
            if self.healthy.load(Ordering::Relaxed) {
                Ok(())
            } else {
                Err(PortError::Unavailable("simulated db outage".to_owned()))
            }
        })
    }
}

/// Test-friendly [`MailSender`] capturing every outgoing `(to_email, login_url)`.
#[derive(Debug, Default)]
pub struct CaptureMailSender {
    sent: Mutex<Vec<(String, String)>>,
}

impl CaptureMailSender {
    /// Returns all captured `(email, login_url)` pairs.
    #[must_use]
    pub fn sent_messages(&self) -> Vec<(String, String)> {
        self.sent.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl MailSender for CaptureMailSender {
    fn send_magic_link(
        &self,
        to_email: &str,
        login_url: &str,
    ) -> BoxFuture<'_, Result<(), PortError>> {
        let to = to_email.to_owned();
        let url = login_url.to_owned();
        Box::pin(async move {
            let mut guard = self
                .sent
                .lock()
                .map_err(|_| PortError::Unavailable("lock poisoned".to_owned()))?;
            guard.push((to, url));
            Ok(())
        })
    }
}

/// Test-friendly [`GoogleTokenVerifier`] accepting `"valid:<sub_id>:<email>"` tokens.
#[derive(Debug, Default)]
pub struct TestGoogleVerifier;

impl GoogleTokenVerifier for TestGoogleVerifier {
    fn verify_id_token(
        &self,
        id_token: &str,
    ) -> BoxFuture<'_, Result<VerifiedGoogleUser, PortError>> {
        let token = id_token.to_owned();
        Box::pin(async move {
            let mut parts = token.splitn(3, ':');
            if let (Some("valid"), Some(sub), Some(email)) =
                (parts.next(), parts.next(), parts.next())
            {
                return Ok(VerifiedGoogleUser {
                    sub: sub.to_owned(),
                    email: email.to_owned(),
                    suggested_name: None,
                });
            }
            Err(PortError::Invalid("invalid Google ID token".to_owned()))
        })
    }
}
