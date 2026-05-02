// Session cookies + Axum extractors for the current user.
//
// Sessions live in the `sessions` DB table. The cookie carries an opaque
// random token; we store its sha256 hash, so reading the DB doesn't reveal
// active session tokens. Lifetime is fixed (no sliding expiration).

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap, StatusCode},
    response::IntoResponse,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Utc};

use crate::db::{NewSession, Storage, UserRecord};
use crate::state::AppState;

use super::{new_token, sha256_hex, AuthError};

pub const COOKIE_NAME: &str = "cotuong_session";
pub const SESSION_TTL: Duration = Duration::days(30);

/// Build the Set-Cookie for a freshly minted session.
pub fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    let mut c = Cookie::new(COOKIE_NAME, token);
    c.set_http_only(true);
    c.set_path("/");
    c.set_same_site(SameSite::Lax);
    c.set_secure(secure);
    c.set_max_age(time::Duration::seconds(SESSION_TTL.num_seconds()));
    c
}

/// Build the Set-Cookie that clears the session on the browser side.
pub fn cleared_cookie() -> Cookie<'static> {
    let mut c = Cookie::new(COOKIE_NAME, "");
    c.set_http_only(true);
    c.set_path("/");
    c.set_same_site(SameSite::Lax);
    c.set_max_age(time::Duration::seconds(0));
    c
}

/// Issue a new session for `user` and return (raw_token, set_cookie).
pub async fn issue_session(
    storage: &Arc<dyn Storage>,
    user_id: uuid::Uuid,
    user_agent: Option<String>,
    secure: bool,
) -> Result<Cookie<'static>, AuthError> {
    let (raw, hash) = new_token();
    storage
        .create_session(NewSession {
            token_hash: hash,
            user_id,
            expires_at: Utc::now() + SESSION_TTL,
            user_agent,
        })
        .await?;
    Ok(session_cookie(raw, secure))
}

/// Read the session cookie and resolve it to a user. Returns `None` cleanly
/// when the cookie is missing or stale, so callers can implement either
/// optional or required auth on top.
pub async fn user_from_cookies(
    storage: &Arc<dyn Storage>,
    jar: &CookieJar,
) -> Option<UserRecord> {
    let token = jar.get(COOKIE_NAME)?.value().to_string();
    if token.is_empty() {
        return None;
    }
    let hash = sha256_hex(&token);
    storage.get_session_user(&hash).await.ok().flatten()
}

/// Same as `user_from_cookies` but reads cookies straight from headers.
/// Used by the WebSocket upgrade handler, which doesn't take a CookieJar
/// extractor since it's already mid-extraction.
pub async fn user_from_headers(
    storage: &Arc<dyn Storage>,
    headers: &HeaderMap,
) -> Option<UserRecord> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for kv in cookie_header.split(';') {
        let kv = kv.trim();
        if let Some((k, v)) = kv.split_once('=') {
            if k == COOKIE_NAME && !v.is_empty() {
                let hash = sha256_hex(v);
                return storage.get_session_user(&hash).await.ok().flatten();
            }
        }
    }
    None
}

// ---- Extractors ------------------------------------------------------------

/// Required-auth extractor. Responds 401 if no valid session.
pub struct CurrentUser(pub UserRecord);

#[async_trait::async_trait]
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AuthRejection;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        match user_from_cookies(&state.storage, &jar).await {
            Some(u) => Ok(CurrentUser(u)),
            None => Err(AuthRejection::Unauthorized),
        }
    }
}

/// Optional-auth extractor. Always succeeds, may yield `None`.
#[allow(dead_code)] // available for future handlers that opt to support both
pub struct OptionalUser(pub Option<UserRecord>);

#[async_trait::async_trait]
impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        Ok(OptionalUser(user_from_cookies(&state.storage, &jar).await))
    }
}

pub enum AuthRejection {
    Unauthorized,
}

impl IntoResponse for AuthRejection {
    fn into_response(self) -> axum::response::Response {
        match self {
            AuthRejection::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "not authenticated").into_response()
            }
        }
    }
}
