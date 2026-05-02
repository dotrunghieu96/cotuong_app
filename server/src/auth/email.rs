// SMTP-backed verification + password-reset emails.
// Compiled only when the `email` feature is on, and only meaningful when
// COTUONG_EMAIL_VERIFY is set or password-reset endpoints are hit.

use std::sync::Arc;

use chrono::{Duration, Utc};
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::db::{EmailTokenPurpose, NewEmailToken, Storage, UserRecord};
use crate::state::AuthConfig;

use super::{new_token, AuthError};

const VERIFY_TTL: Duration = Duration::days(2);
const RESET_TTL: Duration = Duration::hours(1);

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: Mailbox,
}

impl SmtpConfig {
    /// Pull SMTP settings from env. Returns `None` if any required var is
    /// missing — the caller surfaces a startup error in that case.
    pub fn from_env() -> Option<Self> {
        let host = std::env::var("COTUONG_SMTP_HOST").ok()?;
        let port: u16 = std::env::var("COTUONG_SMTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(587);
        let username = std::env::var("COTUONG_SMTP_USERNAME").ok()?;
        let password = std::env::var("COTUONG_SMTP_PASSWORD").ok()?;
        let from_str = std::env::var("COTUONG_SMTP_FROM").ok()?;
        let from: Mailbox = from_str.parse().ok()?;
        Some(Self {
            host,
            port,
            username,
            password,
            from,
        })
    }

    fn transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error>
    {
        let creds = Credentials::new(self.username.clone(), self.password.clone());
        Ok(AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)?
            .port(self.port)
            .credentials(creds)
            .build())
    }
}

async fn send(cfg: &SmtpConfig, to: Mailbox, subject: &str, body: String) -> Result<(), AuthError> {
    let msg = Message::builder()
        .from(cfg.from.clone())
        .to(to)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| {
            tracing::warn!("email build failed: {e}");
            AuthError::Internal
        })?;
    let transport = cfg.transport().map_err(|e| {
        tracing::warn!("smtp transport build failed: {e}");
        AuthError::Internal
    })?;
    transport.send(msg).await.map_err(|e| {
        tracing::warn!("smtp send failed: {e}");
        AuthError::Internal
    })?;
    Ok(())
}

pub async fn send_verify_email(
    storage: &Arc<dyn Storage>,
    cfg: &AuthConfig,
    user: &UserRecord,
) -> Result<(), AuthError> {
    let Some(smtp) = SmtpConfig::from_env() else {
        tracing::warn!("verification requested but SMTP not configured");
        return Err(AuthError::Internal);
    };
    let (raw, hash) = new_token();
    storage
        .create_email_token(NewEmailToken {
            token_hash: hash,
            user_id: user.id,
            purpose: EmailTokenPurpose::VerifyEmail,
            expires_at: Utc::now() + VERIFY_TTL,
        })
        .await?;
    let url = format!(
        "{}/auth/verify/{}",
        cfg.public_base_url.trim_end_matches('/'),
        raw
    );
    let to: Mailbox = format!("{} <{}>", user.username, user.email)
        .parse()
        .map_err(|_| AuthError::Internal)?;
    let body = format!(
        "Welcome to cờ tướng, {}!\n\nClick to verify your email:\n{url}\n\nThis link is good for 48 hours.",
        user.username
    );
    send(&smtp, to, "Verify your cờ tướng email", body).await
}

pub async fn send_reset_email(
    storage: &Arc<dyn Storage>,
    cfg: &AuthConfig,
    user: &UserRecord,
) -> Result<(), AuthError> {
    let Some(smtp) = SmtpConfig::from_env() else {
        tracing::warn!("password reset requested but SMTP not configured");
        return Err(AuthError::Internal);
    };
    let (raw, hash) = new_token();
    storage
        .create_email_token(NewEmailToken {
            token_hash: hash,
            user_id: user.id,
            purpose: EmailTokenPurpose::PasswordReset,
            expires_at: Utc::now() + RESET_TTL,
        })
        .await?;
    let url = format!(
        "{}/auth/password-reset/confirm?token={}",
        cfg.public_base_url.trim_end_matches('/'),
        raw
    );
    let to: Mailbox = format!("{} <{}>", user.username, user.email)
        .parse()
        .map_err(|_| AuthError::Internal)?;
    let body = format!(
        "Someone (hopefully you) asked to reset your cờ tướng password.\n\nClick to set a new one:\n{url}\n\nThis link is good for 1 hour. If you didn't ask, ignore this email.",
    );
    send(&smtp, to, "Reset your cờ tướng password", body).await
}
