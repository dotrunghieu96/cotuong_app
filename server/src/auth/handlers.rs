// HTTP handlers for password auth: signup, login, logout, me.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::db::{NewUser, UserRecord};
use crate::state::AppState;

use super::session::{cleared_cookie, issue_session, CurrentUser};
use super::{
    hash_password, validate_email, validate_password, validate_username, verify_password,
    AuthError,
};

// ---- Wire types ------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Username OR email.
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserView {
    pub id: uuid::Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
}

impl From<UserRecord> for UserView {
    fn from(u: UserRecord) -> Self {
        Self {
            id: u.id,
            username: u.username,
            email: u.email,
            email_verified: u.email_verified,
        }
    }
}

// ---- Handlers --------------------------------------------------------------

pub async fn signup(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SignupRequest>,
) -> Result<(CookieJar, Json<UserView>), AuthError> {
    validate_username(&req.username)?;
    validate_email(&req.email)?;
    validate_password(&req.password)?;

    let password_hash = hash_password(&req.password)?;

    // Mark verified up front when verification is disabled — no point storing
    // a perpetual "unverified" flag we never plan to clear.
    let email_verified = !state.cfg.require_email_verification;

    let user = state
        .storage
        .create_user(NewUser {
            username: req.username,
            email: req.email,
            password_hash: Some(password_hash),
            oauth_provider: None,
            oauth_subject: None,
            email_verified,
        })
        .await
        .map_err(map_storage_conflict)?;

    if state.cfg.require_email_verification {
        #[cfg(feature = "email")]
        {
            if let Err(e) =
                super::email::send_verify_email(&state.storage, &*state.cfg, &user).await
            {
                tracing::warn!("send_verify_email failed: {e}");
            }
        }
    }

    let cookie = issue_session(&state.storage, user.id, None, state.cfg.cookie_secure).await?;
    Ok((jar.add(cookie), Json(user.into())))
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserView>), AuthError> {
    let user = state
        .storage
        .get_user_by_username_or_email(&req.identifier)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let Some(hash) = user.password_hash.as_deref() else {
        // OAuth-only account; password login isn't valid for this user.
        return Err(AuthError::InvalidCredentials);
    };

    if !verify_password(&req.password, hash) {
        return Err(AuthError::InvalidCredentials);
    }

    if state.cfg.require_email_verification && !user.email_verified {
        return Err(AuthError::InvalidInput {
            field: "email",
            reason: "not verified — check your inbox",
        });
    }

    let _ = state.storage.touch_last_seen(user.id).await;
    let cookie = issue_session(&state.storage, user.id, None, state.cfg.cookie_secure).await?;
    Ok((jar.add(cookie), Json(user.into())))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if let Some(c) = jar.get(super::session::COOKIE_NAME) {
        let hash = super::sha256_hex(c.value());
        let _ = state.storage.delete_session(&hash).await;
    }
    (jar.add(cleared_cookie()), StatusCode::NO_CONTENT)
}

pub async fn me(CurrentUser(user): CurrentUser) -> Json<UserView> {
    Json(user.into())
}

// ---- Email verification + password reset ---------------------------------

pub async fn verify_email(
    State(state): State<AppState>,
    axum::extract::Path(token): axum::extract::Path<String>,
) -> Result<axum::response::Redirect, AuthError> {
    let hash = super::sha256_hex(&token);
    let user_id = state
        .storage
        .consume_email_token(&hash, crate::db::EmailTokenPurpose::VerifyEmail)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;
    state.storage.set_email_verified(user_id).await?;
    Ok(axum::response::Redirect::to("/?verified=1"))
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

pub async fn password_reset_request(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetRequest>,
) -> impl IntoResponse {
    // Always return 204 so we don't leak which emails are registered.
    if let Ok(Some(user)) = state.storage.get_user_by_email(&req.email).await {
        #[cfg(feature = "email")]
        {
            if let Err(e) =
                super::email::send_reset_email(&state.storage, &*state.cfg, &user).await
            {
                tracing::warn!("send_reset_email failed: {e}");
            }
        }
        #[cfg(not(feature = "email"))]
        {
            let _ = user; // unused without email feature
            tracing::info!("password reset requested but `email` feature is off");
        }
    }
    StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetConfirm {
    pub token: String,
    pub password: String,
}

pub async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirm>,
) -> Result<StatusCode, AuthError> {
    super::validate_password(&req.password)?;
    let hash = super::sha256_hex(&req.token);
    let user_id = state
        .storage
        .consume_email_token(&hash, crate::db::EmailTokenPurpose::PasswordReset)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;
    let new_hash = hash_password(&req.password)?;
    state.storage.set_password_hash(user_id, &new_hash).await?;
    // Best-effort: a thoughtful UX would invalidate all of this user's existing
    // sessions here. We don't currently expose that on the trait — TODO.
    Ok(StatusCode::NO_CONTENT)
}

// ---- Error mapping ---------------------------------------------------------

fn map_storage_conflict(e: crate::db::StorageError) -> AuthError {
    match e {
        crate::db::StorageError::Conflict(field) => AuthError::Conflict(field),
        other => AuthError::Storage(other),
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        // Keep error bodies small + generic — clients pick by status + `error` tag.
        let (status, code, message): (StatusCode, &str, String) = match self {
            AuthError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "invalid credentials".into(),
            ),
            AuthError::InvalidInput { field, reason } => (
                StatusCode::BAD_REQUEST,
                "invalid_input",
                format!("{field}: {reason}"),
            ),
            AuthError::Conflict(field) => (
                StatusCode::CONFLICT,
                "conflict",
                format!("{field} already in use"),
            ),
            AuthError::NotAuthenticated => (
                StatusCode::UNAUTHORIZED,
                "not_authenticated",
                "not authenticated".into(),
            ),
            AuthError::Storage(e) => {
                tracing::warn!("auth storage error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".into(),
                )
            }
            AuthError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "internal error".into(),
            ),
        };
        let body = serde_json::json!({"error": code, "message": message});
        (status, Json(body)).into_response()
    }
}

