// Periodic SQLite -> S3-compatible (Cloudflare R2) backup.
//
// Activated when COTUONG_R2_BUCKET is set. Every `interval` seconds we run
// SQLite's `VACUUM INTO` to a temp file (a consistent, compacted snapshot
// without blocking writers for long), upload it under
// `<prefix>/cotuong-<timestamp>.db`, and delete the temp file.
//
// Postgres is intentionally out of scope: pg_dump + cron is the well-trodden
// path there and doing it in-process loses too many knobs.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use chrono::Utc;
use sqlx::SqlitePool;

use super::sqlite::SqliteStorage;

#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub bucket: String,
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub key_prefix: String,
    pub interval: Duration,
}

impl BackupConfig {
    /// Read config from environment. Returns `None` if `COTUONG_R2_BUCKET`
    /// is unset, signaling that backup is disabled.
    pub fn from_env() -> Option<Self> {
        let bucket = std::env::var("COTUONG_R2_BUCKET").ok()?;
        let endpoint = std::env::var("COTUONG_R2_ENDPOINT").ok()?;
        let access_key_id = std::env::var("COTUONG_R2_ACCESS_KEY_ID").ok()?;
        let secret_access_key = std::env::var("COTUONG_R2_SECRET_ACCESS_KEY").ok()?;
        let region = std::env::var("COTUONG_R2_REGION").unwrap_or_else(|_| "auto".to_string());
        let key_prefix = std::env::var("COTUONG_R2_PREFIX").unwrap_or_else(|_| "cotuong".to_string());
        let interval = std::env::var("COTUONG_R2_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3600);
        Some(Self {
            bucket,
            endpoint,
            region,
            access_key_id,
            secret_access_key,
            key_prefix,
            interval: Duration::from_secs(interval),
        })
    }
}

async fn build_client(cfg: &BackupConfig) -> Client {
    let creds = Credentials::new(
        &cfg.access_key_id,
        &cfg.secret_access_key,
        None,
        None,
        "cotuong-r2",
    );
    let shared = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(cfg.region.clone()))
        .endpoint_url(cfg.endpoint.clone())
        .credentials_provider(creds)
        .load()
        .await;
    let s3 = aws_sdk_s3::config::Builder::from(&shared)
        .force_path_style(true)
        .build();
    Client::from_conf(s3)
}

/// Spawn the background task. Returns immediately; the task lives until the
/// runtime shuts down. Config / connectivity errors are logged and retried
/// on the next interval — backup failures must not break live play.
pub fn spawn(storage: Arc<SqliteStorage>, cfg: BackupConfig) {
    let Some(file_path) = storage.file_path().map(PathBuf::from) else {
        tracing::warn!("R2 backup configured but sqlite is in-memory; backup disabled");
        return;
    };
    tokio::spawn(async move {
        let client = build_client(&cfg).await;
        let mut ticker = tokio::time::interval(cfg.interval);
        // Skip the first immediate tick; first backup runs after `interval`.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = run_once(storage.pool(), &file_path, &client, &cfg).await {
                tracing::warn!("R2 backup failed: {e}");
            }
        }
    });
}

async fn run_once(
    pool: &SqlitePool,
    file_path: &std::path::Path,
    client: &Client,
    cfg: &BackupConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let snapshot_name = format!(
        "{}-{stamp}.db",
        file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cotuong")
    );
    let snapshot_path = std::env::temp_dir().join(&snapshot_name);

    // Best-effort cleanup if a previous run left this around.
    let _ = tokio::fs::remove_file(&snapshot_path).await;

    let snapshot_str = snapshot_path.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{}'", snapshot_str))
        .execute(pool)
        .await?;

    let key = format!("{}/{}", cfg.key_prefix.trim_end_matches('/'), snapshot_name);
    let body = ByteStream::from_path(&snapshot_path).await?;

    client
        .put_object()
        .bucket(&cfg.bucket)
        .key(&key)
        .body(body)
        .send()
        .await?;

    let _ = tokio::fs::remove_file(&snapshot_path).await;
    tracing::info!("R2 backup uploaded: s3://{}/{}", cfg.bucket, key);
    Ok(())
}
