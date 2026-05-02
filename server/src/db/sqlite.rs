use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::{
    EmailTokenPurpose, GameRecord, GameResult, ListGamesQuery, MoveRecord, NewEmailToken,
    NewSession, NewUser, Result, Storage, StorageError, Termination, UserRecord,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

pub struct SqliteStorage {
    pool: SqlitePool,
    /// Path to the on-disk database file (None for in-memory). Used by R2 backup.
    file_path: Option<std::path::PathBuf>,
}

impl SqliteStorage {
    pub async fn connect(url: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));

        let file_path = sqlite_file_path(url);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;

        MIGRATOR.run(&pool).await?;

        Ok(Self { pool, file_path })
    }

    #[allow(dead_code)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[allow(dead_code)]
    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.file_path.as_deref()
    }

    async fn classify_user_conflict(&self, u: &NewUser) -> StorageError {
        // Disambiguate which uniqueness constraint failed. Best-effort: a
        // racing insert could change the picture between these two queries,
        // but the resulting message is still in the right ballpark.
        if let Ok(Some(_)) = self.get_user_by_username_or_email(&u.username).await {
            return StorageError::Conflict("username");
        }
        if let Ok(Some(_)) = self.get_user_by_email(&u.email).await {
            return StorageError::Conflict("email");
        }
        StorageError::Conflict("user")
    }
}

fn sqlite_file_path(url: &str) -> Option<std::path::PathBuf> {
    let s = url.strip_prefix("sqlite://").or_else(|| url.strip_prefix("sqlite:"))?;
    if s.is_empty() || s == ":memory:" || s.starts_with(":memory:") {
        return None;
    }
    let bare = s.split('?').next().unwrap_or(s);
    if bare.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(bare))
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e {
        // SQLite's extended error codes via sqlite3_extended_result_codes:
        // 2067 = SQLITE_CONSTRAINT_UNIQUE, 1555 = SQLITE_CONSTRAINT_PRIMARYKEY
        if let Some(code) = db_err.code() {
            return code == "2067" || code == "1555";
        }
    }
    false
}

#[async_trait]
impl Storage for SqliteStorage {
    // ---- Games ------------------------------------------------------------

    async fn create_game(
        &self,
        room_code: &str,
        red_user_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO games (id, room_code, started_at, red_user_id) VALUES (?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(room_code)
        .bind(now)
        .bind(red_user_id.map(|u| u.to_string()))
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    async fn set_black_player(&self, game_id: Uuid, user_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE games SET black_user_id = ? WHERE id = ? AND black_user_id IS NULL")
            .bind(user_id.to_string())
            .bind(game_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn record_move(
        &self,
        game_id: Uuid,
        ply: i32,
        from_sq: i32,
        to_sq: i32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO moves (game_id, ply, from_sq, to_sq, played_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(game_id.to_string())
        .bind(ply as i64)
        .bind(from_sq as i64)
        .bind(to_sq as i64)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn finish_game(
        &self,
        game_id: Uuid,
        result: Option<GameResult>,
        termination: Termination,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE games SET finished_at = ?, result = ?, termination = ? WHERE id = ?",
        )
        .bind(Utc::now())
        .bind(result.map(|r| r.as_str()))
        .bind(termination.as_str())
        .bind(game_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_game(&self, game_id: Uuid) -> Result<Option<GameRecord>> {
        let row = sqlx::query(LIST_GAMES_BASE_SQL.replace(
            "{WHERE}",
            "WHERE g.id = ? ORDER BY g.started_at DESC LIMIT 1",
        ).as_str())
        .bind(game_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_game).transpose()?)
    }

    async fn list_games(&self, q: ListGamesQuery) -> Result<Vec<GameRecord>> {
        let limit = q.limit.clamp(1, 500) as i64;
        let where_clause = if q.finished_only {
            "WHERE g.finished_at IS NOT NULL ORDER BY g.started_at DESC LIMIT ?"
        } else {
            "ORDER BY g.started_at DESC LIMIT ?"
        };
        let sql = LIST_GAMES_BASE_SQL.replace("{WHERE}", where_clause);
        let rows = sqlx::query(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(row_to_game).collect()
    }

    async fn get_game_for_user(
        &self,
        user_id: Uuid,
        game_id: Uuid,
    ) -> Result<Option<GameRecord>> {
        let sql = LIST_GAMES_BASE_SQL.replace(
            "{WHERE}",
            "WHERE g.id = ? AND (g.red_user_id = ? OR g.black_user_id = ?)
             ORDER BY g.started_at DESC LIMIT 1",
        );
        let uid = user_id.to_string();
        let row = sqlx::query(&sql)
            .bind(game_id.to_string())
            .bind(&uid)
            .bind(&uid)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(row_to_game).transpose()?)
    }

    async fn list_games_for_user(
        &self,
        user_id: Uuid,
        q: ListGamesQuery,
    ) -> Result<Vec<GameRecord>> {
        let limit = q.limit.clamp(1, 500) as i64;
        let where_clause = if q.finished_only {
            "WHERE (g.red_user_id = ? OR g.black_user_id = ?)
                AND g.finished_at IS NOT NULL
             ORDER BY g.started_at DESC LIMIT ?"
        } else {
            "WHERE (g.red_user_id = ? OR g.black_user_id = ?)
             ORDER BY g.started_at DESC LIMIT ?"
        };
        let sql = LIST_GAMES_BASE_SQL.replace("{WHERE}", where_clause);
        let uid = user_id.to_string();
        let rows = sqlx::query(&sql)
            .bind(&uid)
            .bind(&uid)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(row_to_game).collect()
    }

    async fn list_moves(&self, game_id: Uuid) -> Result<Vec<MoveRecord>> {
        let rows = sqlx::query(
            "SELECT ply, from_sq, to_sq, played_at
             FROM moves WHERE game_id = ? ORDER BY ply",
        )
        .bind(game_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(MoveRecord {
                    ply: r.try_get::<i64, _>("ply")? as i32,
                    from_sq: r.try_get::<i64, _>("from_sq")? as i32,
                    to_sq: r.try_get::<i64, _>("to_sq")? as i32,
                    played_at: r.try_get("played_at")?,
                })
            })
            .collect()
    }

    // ---- Users ------------------------------------------------------------

    async fn create_user(&self, u: NewUser) -> Result<UserRecord> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let res = sqlx::query(
            "INSERT INTO users (id, username, email, email_verified, password_hash,
                                oauth_provider, oauth_subject, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&u.username)
        .bind(&u.email)
        .bind(u.email_verified as i64)
        .bind(&u.password_hash)
        .bind(&u.oauth_provider)
        .bind(&u.oauth_subject)
        .bind(now)
        .execute(&self.pool)
        .await;

        match res {
            Ok(_) => Ok(UserRecord {
                id,
                username: u.username,
                email: u.email,
                email_verified: u.email_verified,
                password_hash: u.password_hash,
                oauth_provider: u.oauth_provider,
                oauth_subject: u.oauth_subject,
                created_at: now,
            }),
            Err(e) if is_unique_violation(&e) => Err(self.classify_user_conflict(&u).await),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_user_by_id(&self, id: Uuid) -> Result<Option<UserRecord>> {
        let row = sqlx::query(SQL_USER_SELECT_BY_ID)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(row_to_user).transpose()?)
    }

    async fn get_user_by_username_or_email(
        &self,
        identifier: &str,
    ) -> Result<Option<UserRecord>> {
        let row = sqlx::query(
            "SELECT id, username, email, email_verified, password_hash,
                    oauth_provider, oauth_subject, created_at
             FROM users WHERE username = ? OR email = ? LIMIT 1",
        )
        .bind(identifier)
        .bind(identifier)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_user).transpose()?)
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let row = sqlx::query(
            "SELECT id, username, email, email_verified, password_hash,
                    oauth_provider, oauth_subject, created_at
             FROM users WHERE email = ? LIMIT 1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_user).transpose()?)
    }

    async fn get_user_by_oauth(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<UserRecord>> {
        let row = sqlx::query(
            "SELECT id, username, email, email_verified, password_hash,
                    oauth_provider, oauth_subject, created_at
             FROM users WHERE oauth_provider = ? AND oauth_subject = ? LIMIT 1",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_user).transpose()?)
    }

    async fn set_email_verified(&self, user_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE users SET email_verified = 1 WHERE id = ?")
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn touch_last_seen(&self, user_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE users SET last_seen_at = ? WHERE id = ?")
            .bind(Utc::now())
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- Sessions ---------------------------------------------------------

    async fn create_session(&self, s: NewSession) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, user_agent)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&s.token_hash)
        .bind(s.user_id.to_string())
        .bind(Utc::now())
        .bind(s.expires_at)
        .bind(&s.user_agent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session_user(&self, token_hash: &str) -> Result<Option<UserRecord>> {
        let row = sqlx::query(
            "SELECT u.id, u.username, u.email, u.email_verified, u.password_hash,
                    u.oauth_provider, u.oauth_subject, u.created_at
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > ?",
        )
        .bind(token_hash)
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_user).transpose()?)
    }

    async fn delete_session(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn prune_expired_sessions(&self) -> Result<u64> {
        let r = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    // ---- Email tokens -----------------------------------------------------

    async fn create_email_token(&self, t: NewEmailToken) -> Result<()> {
        sqlx::query(
            "INSERT INTO email_tokens (token_hash, user_id, purpose, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&t.token_hash)
        .bind(t.user_id.to_string())
        .bind(t.purpose.as_str())
        .bind(Utc::now())
        .bind(t.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn consume_email_token(
        &self,
        token_hash: &str,
        purpose: EmailTokenPurpose,
    ) -> Result<Option<Uuid>> {
        // RETURNING is supported in SQLite ≥ 3.35.
        let row = sqlx::query(
            "UPDATE email_tokens SET used_at = ?
             WHERE token_hash = ? AND purpose = ? AND used_at IS NULL AND expires_at > ?
             RETURNING user_id",
        )
        .bind(Utc::now())
        .bind(token_hash)
        .bind(purpose.as_str())
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            None => Ok(None),
            Some(r) => {
                let s: String = r.try_get("user_id")?;
                let id = Uuid::parse_str(&s)
                    .map_err(|_| sqlx::Error::Decode("user_id parse".into()))?;
                Ok(Some(id))
            }
        }
    }
}

const SQL_USER_SELECT_BY_ID: &str =
    "SELECT id, username, email, email_verified, password_hash,
            oauth_provider, oauth_subject, created_at
     FROM users WHERE id = ?";

// Shared base for game listing/lookup. `{WHERE}` is replaced at call site
// with the filter + ordering + limit clauses.
const LIST_GAMES_BASE_SQL: &str =
    "SELECT g.id, g.room_code, g.started_at, g.finished_at, g.result, g.termination,
            r.username AS red_username, b.username AS black_username
     FROM games g
     LEFT JOIN users r ON g.red_user_id = r.id
     LEFT JOIN users b ON g.black_user_id = b.id
     {WHERE}";

fn row_to_game(r: SqliteRow) -> Result<GameRecord> {
    let id_text: String = r.try_get("id")?;
    let id = Uuid::parse_str(&id_text).map_err(|_| sqlx::Error::Decode("uuid parse".into()))?;
    Ok(GameRecord {
        id,
        room_code: r.try_get("room_code")?,
        started_at: r.try_get::<DateTime<Utc>, _>("started_at")?,
        finished_at: r.try_get::<Option<DateTime<Utc>>, _>("finished_at")?,
        result: r.try_get("result")?,
        termination: r.try_get("termination")?,
        red_player: r.try_get("red_username").ok(),
        black_player: r.try_get("black_username").ok(),
    })
}

fn row_to_user(r: SqliteRow) -> Result<UserRecord> {
    let id_text: String = r.try_get("id")?;
    let id = Uuid::parse_str(&id_text).map_err(|_| sqlx::Error::Decode("uuid parse".into()))?;
    let verified: i64 = r.try_get("email_verified")?;
    Ok(UserRecord {
        id,
        username: r.try_get("username")?,
        email: r.try_get("email")?,
        email_verified: verified != 0,
        password_hash: r.try_get("password_hash")?,
        oauth_provider: r.try_get("oauth_provider")?,
        oauth_subject: r.try_get("oauth_subject")?,
        created_at: r.try_get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh() -> SqliteStorage {
        SqliteStorage::connect("sqlite::memory:")
            .await
            .expect("connect")
    }

    #[tokio::test]
    async fn create_record_finish_roundtrip() {
        let s = fresh().await;
        let id = s.create_game("ABC123", None).await.unwrap();
        s.record_move(id, 1, 75, 65).await.unwrap();
        s.record_move(id, 2, 14, 24).await.unwrap();
        s.finish_game(id, Some(GameResult::RedWins), Termination::Checkmate)
            .await
            .unwrap();

        let game = s.get_game(id).await.unwrap().expect("game");
        assert_eq!(game.room_code, "ABC123");
        assert_eq!(game.result.as_deref(), Some("red_wins"));
        assert_eq!(game.termination.as_deref(), Some("checkmate"));
        assert!(game.finished_at.is_some());

        let moves = s.list_moves(id).await.unwrap();
        assert_eq!(moves.len(), 2);
    }

    #[tokio::test]
    async fn list_games_orders_by_recency_and_filters_finished() {
        let s = fresh().await;
        let g1 = s.create_game("ROOM01", None).await.unwrap();
        let g2 = s.create_game("ROOM02", None).await.unwrap();
        s.finish_game(g1, Some(GameResult::BlackWins), Termination::Resignation)
            .await
            .unwrap();

        let all = s
            .list_games(ListGamesQuery {
                limit: 10,
                finished_only: false,
            })
            .await
            .unwrap();
        assert_eq!(all.len(), 2);

        let finished = s
            .list_games(ListGamesQuery {
                limit: 10,
                finished_only: true,
            })
            .await
            .unwrap();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].id, g1);
        let _ = g2;
    }

    #[tokio::test]
    async fn abandoned_game_has_null_result() {
        let s = fresh().await;
        let id = s.create_game("ABAND1", None).await.unwrap();
        s.finish_game(id, None, Termination::Abandoned).await.unwrap();
        let game = s.get_game(id).await.unwrap().expect("game");
        assert!(game.result.is_none());
        assert_eq!(game.termination.as_deref(), Some("abandoned"));
    }

    fn new_user(name: &str) -> NewUser {
        NewUser {
            username: name.into(),
            email: format!("{}@example.com", name),
            password_hash: Some(format!("hash_for_{name}")),
            oauth_provider: None,
            oauth_subject: None,
            email_verified: false,
        }
    }

    #[tokio::test]
    async fn user_create_and_lookup() {
        let s = fresh().await;
        let u = s.create_user(new_user("alice")).await.unwrap();
        let by_id = s.get_user_by_id(u.id).await.unwrap().unwrap();
        assert_eq!(by_id.username, "alice");

        let by_username = s.get_user_by_username_or_email("alice").await.unwrap().unwrap();
        assert_eq!(by_username.id, u.id);

        let by_email = s
            .get_user_by_username_or_email("alice@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_email.id, u.id);
    }

    #[tokio::test]
    async fn user_lookup_is_case_insensitive() {
        let s = fresh().await;
        s.create_user(new_user("Alice")).await.unwrap();
        let by_username = s.get_user_by_username_or_email("alice").await.unwrap();
        assert!(by_username.is_some(), "username lookup should be case-insensitive");
        let by_email = s.get_user_by_email("ALICE@example.com").await.unwrap();
        assert!(by_email.is_some(), "email lookup should be case-insensitive");
    }

    #[tokio::test]
    async fn duplicate_username_and_email_conflict() {
        let s = fresh().await;
        s.create_user(new_user("alice")).await.unwrap();
        let dup_username = s
            .create_user(NewUser {
                email: "other@example.com".into(),
                ..new_user("alice")
            })
            .await;
        match dup_username {
            Err(StorageError::Conflict(field)) => assert_eq!(field, "username"),
            other => panic!("expected username conflict, got {other:?}"),
        }
        let dup_email = s
            .create_user(NewUser {
                username: "bob".into(),
                ..new_user("alice")
            })
            .await;
        match dup_email {
            Err(StorageError::Conflict(field)) => assert_eq!(field, "email"),
            other => panic!("expected email conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn session_create_and_lookup() {
        let s = fresh().await;
        let u = s.create_user(new_user("alice")).await.unwrap();
        s.create_session(NewSession {
            token_hash: "abc123".into(),
            user_id: u.id,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            user_agent: Some("test".into()),
        })
        .await
        .unwrap();
        let got = s.get_session_user("abc123").await.unwrap().unwrap();
        assert_eq!(got.id, u.id);

        s.delete_session("abc123").await.unwrap();
        assert!(s.get_session_user("abc123").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn expired_session_is_invisible_and_pruned() {
        let s = fresh().await;
        let u = s.create_user(new_user("alice")).await.unwrap();
        s.create_session(NewSession {
            token_hash: "expired".into(),
            user_id: u.id,
            expires_at: Utc::now() - chrono::Duration::hours(1),
            user_agent: None,
        })
        .await
        .unwrap();
        assert!(s.get_session_user("expired").await.unwrap().is_none());
        let pruned = s.prune_expired_sessions().await.unwrap();
        assert_eq!(pruned, 1);
    }

    #[tokio::test]
    async fn email_token_consume_is_one_shot() {
        let s = fresh().await;
        let u = s.create_user(new_user("alice")).await.unwrap();
        s.create_email_token(NewEmailToken {
            token_hash: "tk1".into(),
            user_id: u.id,
            purpose: EmailTokenPurpose::VerifyEmail,
            expires_at: Utc::now() + chrono::Duration::hours(1),
        })
        .await
        .unwrap();
        let got = s
            .consume_email_token("tk1", EmailTokenPurpose::VerifyEmail)
            .await
            .unwrap();
        assert_eq!(got, Some(u.id));
        // Second consume returns None.
        let again = s
            .consume_email_token("tk1", EmailTokenPurpose::VerifyEmail)
            .await
            .unwrap();
        assert!(again.is_none());
    }

    #[tokio::test]
    async fn games_attribute_to_users() {
        let s = fresh().await;
        let red = s.create_user(new_user("redplayer")).await.unwrap();
        let black = s.create_user(new_user("blackplayer")).await.unwrap();
        let game = s.create_game("ROOMUP", Some(red.id)).await.unwrap();
        s.set_black_player(game, black.id).await.unwrap();
        // Smoke: row exists. (We don't surface user ids in GameRecord yet.)
        assert!(s.get_game(game).await.unwrap().is_some());
    }
}
