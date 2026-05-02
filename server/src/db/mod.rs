// Storage abstraction for game persistence.
//
// SQLite-backed; the trait abstraction is kept so a second backend can be
// added later (originally Postgres was planned, now descoped). The Hub
// holds an `Arc<dyn Storage>` and writes through on game lifecycle events.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

pub mod sqlite;

#[cfg(feature = "r2-backup")]
pub mod backup;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("unsupported database url scheme: {0}")]
    UnsupportedScheme(String),
    #[error("conflict: {0}")]
    Conflict(&'static str),
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Draw isn't produced yet (no repetition rule); reserved.
pub enum GameResult {
    RedWins,
    BlackWins,
    Draw,
}

impl GameResult {
    pub fn as_str(self) -> &'static str {
        match self {
            GameResult::RedWins => "red_wins",
            GameResult::BlackWins => "black_wins",
            GameResult::Draw => "draw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    Checkmate,
    Resignation,
    Abandoned,
}

impl Termination {
    pub fn as_str(self) -> &'static str {
        match self {
            Termination::Checkmate => "checkmate",
            Termination::Resignation => "resignation",
            Termination::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GameRecord {
    pub id: Uuid,
    pub room_code: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
    pub termination: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoveRecord {
    pub ply: i32,
    pub from_sq: i32,
    pub to_sq: i32,
    pub played_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct ListGamesQuery {
    pub limit: i32,
    pub finished_only: bool,
}

impl Default for ListGamesQuery {
    fn default() -> Self {
        Self {
            limit: 50,
            finished_only: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub oauth_provider: Option<String>,
    #[serde(skip)]
    pub oauth_subject: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub oauth_provider: Option<String>,
    pub oauth_subject: Option<String>,
    pub email_verified: bool,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub token_hash: String,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailTokenPurpose {
    VerifyEmail,
    PasswordReset,
}

impl EmailTokenPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            EmailTokenPurpose::VerifyEmail => "verify_email",
            EmailTokenPurpose::PasswordReset => "password_reset",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewEmailToken {
    pub token_hash: String,
    pub user_id: Uuid,
    pub purpose: EmailTokenPurpose,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // ---- Games / moves ----------------------------------------------------
    async fn create_game(
        &self,
        room_code: &str,
        red_user_id: Option<Uuid>,
    ) -> Result<Uuid>;
    async fn set_black_player(&self, game_id: Uuid, user_id: Uuid) -> Result<()>;
    async fn record_move(
        &self,
        game_id: Uuid,
        ply: i32,
        from_sq: i32,
        to_sq: i32,
    ) -> Result<()>;
    async fn finish_game(
        &self,
        game_id: Uuid,
        result: Option<GameResult>,
        termination: Termination,
    ) -> Result<()>;

    async fn get_game(&self, game_id: Uuid) -> Result<Option<GameRecord>>;
    async fn list_games(&self, q: ListGamesQuery) -> Result<Vec<GameRecord>>;
    async fn list_moves(&self, game_id: Uuid) -> Result<Vec<MoveRecord>>;

    // ---- Users ------------------------------------------------------------
    async fn create_user(&self, u: NewUser) -> Result<UserRecord>;
    async fn get_user_by_id(&self, id: Uuid) -> Result<Option<UserRecord>>;
    async fn get_user_by_username_or_email(&self, identifier: &str) -> Result<Option<UserRecord>>;
    async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>>;
    async fn get_user_by_oauth(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<UserRecord>>;
    async fn set_email_verified(&self, user_id: Uuid) -> Result<()>;
    async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<()>;
    async fn touch_last_seen(&self, user_id: Uuid) -> Result<()>;

    // ---- Sessions ---------------------------------------------------------
    async fn create_session(&self, s: NewSession) -> Result<()>;
    /// Return the user a session points at, if the session exists and is unexpired.
    async fn get_session_user(&self, token_hash: &str) -> Result<Option<UserRecord>>;
    async fn delete_session(&self, token_hash: &str) -> Result<()>;
    async fn prune_expired_sessions(&self) -> Result<u64>;

    // ---- Email tokens (verify + reset) -----------------------------------
    async fn create_email_token(&self, t: NewEmailToken) -> Result<()>;
    /// Mark the token used and return the user_id, only if unused and unexpired.
    async fn consume_email_token(
        &self,
        token_hash: &str,
        purpose: EmailTokenPurpose,
    ) -> Result<Option<Uuid>>;
}

/// Connect to the storage backend selected by `url`'s scheme.
///
/// - `sqlite:...` or `sqlite://...` -> SQLite
pub async fn connect(url: &str) -> Result<std::sync::Arc<dyn Storage>> {
    if url.starts_with("sqlite:") {
        return Ok(std::sync::Arc::new(sqlite::SqliteStorage::connect(url).await?));
    }
    Err(StorageError::UnsupportedScheme(
        url.split(':').next().unwrap_or("").to_string(),
    ))
}
