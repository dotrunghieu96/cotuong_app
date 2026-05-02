// Google OAuth 2.0 (OpenID Connect) flow.
//
// /auth/google/login: redirect to Google with state + PKCE, the temp values
//   round-trip through a short-lived signed cookie.
// /auth/google/callback: verify state, exchange code, fetch userinfo, upsert
//   user (matching on (provider, subject)), issue a session.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    TokenUrl,
};
use serde::Deserialize;

use crate::db::{NewUser, Storage, UserRecord};
use crate::state::AppState;

use super::session::issue_session;
use super::{validate_username, AuthError};

const PROVIDER: &str = "google";
const STATE_COOKIE: &str = "cotuong_oauth_state";

#[derive(Debug, Clone)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
}

impl GoogleConfig {
    pub fn from_env(public_base_url: &str) -> Option<Self> {
        let client_id = std::env::var("COTUONG_GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("COTUONG_GOOGLE_CLIENT_SECRET").ok()?;
        let redirect_url = format!(
            "{}/auth/google/callback",
            public_base_url.trim_end_matches('/')
        );
        Some(Self {
            client_id,
            client_secret,
            redirect_url,
        })
    }

    fn client(&self) -> Result<BasicClient, AuthError> {
        let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into())
            .map_err(|_| AuthError::Internal)?;
        let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".into())
            .map_err(|_| AuthError::Internal)?;
        let redirect_url = RedirectUrl::new(self.redirect_url.clone())
            .map_err(|_| AuthError::Internal)?;
        Ok(BasicClient::new(
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
            auth_url,
            Some(token_url),
        )
        .set_redirect_uri(redirect_url))
    }
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    let Some(cfg) = GoogleConfig::from_env(&state.cfg.public_base_url) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Google OAuth is not configured (set COTUONG_GOOGLE_CLIENT_ID + COTUONG_GOOGLE_CLIENT_SECRET).",
        )
            .into_response();
    };
    match login_inner(state, jar, cfg).await {
        Ok((jar, redirect)) => (jar, redirect).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn login_inner(
    state: AppState,
    jar: CookieJar,
    cfg: GoogleConfig,
) -> Result<(CookieJar, Redirect), AuthError> {
    let client = cfg.client()?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("email".into()))
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Round-trip state + verifier through a short-lived cookie.
    let value = format!("{}:{}", csrf_token.secret(), pkce_verifier.secret());
    let mut c = Cookie::new(STATE_COOKIE, value);
    c.set_http_only(true);
    c.set_path("/");
    c.set_same_site(SameSite::Lax);
    c.set_secure(state.cfg.cookie_secure);
    c.set_max_age(time::Duration::minutes(10));

    Ok((jar.add(c), Redirect::to(auth_url.as_str())))
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

pub async fn callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(p): Query<CallbackParams>,
) -> Result<(CookieJar, Redirect), AuthError> {
    if let Some(err) = p.error {
        tracing::info!("oauth user-cancelled or error: {err}");
        return Err(AuthError::InvalidCredentials);
    }
    let code = p.code.ok_or(AuthError::InvalidCredentials)?;
    let returned_state = p.state.ok_or(AuthError::InvalidCredentials)?;

    let cookie = jar.get(STATE_COOKIE).ok_or(AuthError::InvalidCredentials)?;
    let (csrf, verifier) = cookie
        .value()
        .split_once(':')
        .ok_or(AuthError::InvalidCredentials)?;
    if !constant_time_eq(returned_state.as_bytes(), csrf.as_bytes()) {
        return Err(AuthError::InvalidCredentials);
    }
    let verifier = PkceCodeVerifier::new(verifier.to_string());

    let cfg = GoogleConfig::from_env(&state.cfg.public_base_url).ok_or(AuthError::Internal)?;
    let client = cfg.client()?;

    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| {
            tracing::warn!("oauth token exchange failed: {e}");
            AuthError::InvalidCredentials
        })?;

    let userinfo: UserInfo = reqwest::Client::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(token.access_token().secret())
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("userinfo fetch failed: {e}");
            AuthError::Internal
        })?
        .error_for_status()
        .map_err(|e| {
            tracing::warn!("userinfo http error: {e}");
            AuthError::Internal
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::warn!("userinfo decode failed: {e}");
            AuthError::Internal
        })?;

    let user = upsert_oauth_user(&state.storage, &userinfo).await?;
    let _ = state.storage.touch_last_seen(user.id).await;

    let session_cookie =
        issue_session(&state.storage, user.id, None, state.cfg.cookie_secure).await?;
    // Drop the state cookie now that we're done with it.
    let mut cleared = Cookie::new(STATE_COOKIE, "");
    cleared.set_path("/");
    cleared.set_max_age(time::Duration::seconds(0));

    let jar = jar.add(session_cookie).add(cleared);
    Ok((jar, Redirect::to("/")))
}

async fn upsert_oauth_user(
    storage: &Arc<dyn Storage>,
    info: &UserInfo,
) -> Result<UserRecord, AuthError> {
    if let Some(u) = storage.get_user_by_oauth(PROVIDER, &info.sub).await? {
        return Ok(u);
    }
    // First time we've seen this Google subject. If their email already maps
    // to a password account, we *don't* auto-link — that's an attack vector if
    // someone signs up with someone else's email and a Google account ever
    // hooks the same address. Surface a clear conflict instead.
    if storage.get_user_by_email(&info.email).await?.is_some() {
        return Err(AuthError::Conflict("email"));
    }

    let username = pick_username(storage, &info.email, info.name.as_deref()).await?;
    let user = storage
        .create_user(NewUser {
            username,
            email: info.email.clone(),
            password_hash: None,
            oauth_provider: Some(PROVIDER.into()),
            oauth_subject: Some(info.sub.clone()),
            email_verified: info.email_verified,
        })
        .await
        .map_err(|e| match e {
            crate::db::StorageError::Conflict(field) => AuthError::Conflict(field),
            other => AuthError::Storage(other),
        })?;
    Ok(user)
}

async fn pick_username(
    storage: &Arc<dyn Storage>,
    email: &str,
    display: Option<&str>,
) -> Result<String, AuthError> {
    let base = display
        .and_then(sanitize_username)
        .or_else(|| sanitize_username(email.split('@').next().unwrap_or("")))
        .unwrap_or_else(|| "player".to_string());

    // Try base, then base-1, base-2, ... up to N before giving up.
    if validate_username(&base).is_ok()
        && storage
            .get_user_by_username_or_email(&base)
            .await?
            .is_none()
    {
        return Ok(base);
    }
    for n in 1..1000 {
        let candidate = truncate(&format!("{base}-{n}"), 30);
        if validate_username(&candidate).is_ok()
            && storage
                .get_user_by_username_or_email(&candidate)
                .await?
                .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(AuthError::Internal)
}

fn sanitize_username(s: &str) -> Option<String> {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c == '_' || c == '-').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(truncate(&trimmed, 30))
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_punctuation() {
        assert_eq!(sanitize_username("Alice O'Brien").as_deref(), Some("Alice_O_Brien"));
        assert_eq!(
            sanitize_username("alice@example.com").as_deref(),
            Some("alice_example_com")
        );
    }

    #[test]
    fn sanitize_trims_leading_underscores() {
        assert_eq!(sanitize_username("---bob").as_deref(), Some("bob"));
    }

    #[test]
    fn truncate_caps_length() {
        let s = "x".repeat(50);
        assert_eq!(truncate(&s, 30).len(), 30);
    }
}
