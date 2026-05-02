// Shared Axum application state. Passed via `State(AppState)` to handlers
// and unwound into typed sub-states by the `FromRef` impls below so each
// handler only sees what it needs.

use std::sync::Arc;

use axum::extract::FromRef;

use crate::db::Storage;
use crate::hub::Hub;

#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<Hub>,
    pub storage: Arc<dyn Storage>,
    pub cfg: Arc<AuthConfig>,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    /// Set the Secure flag on the session cookie. Off in dev (HTTP), on in
    /// prod (HTTPS).
    pub cookie_secure: bool,
    /// Public base URL of the site (used to build OAuth redirect URIs and
    /// email links). Default: `http://127.0.0.1:8000`.
    pub public_base_url: String,
    /// If true, signup blocks login until the user clicks the verify link.
    /// Off by default; turning it on requires the `email` feature build
    /// AND COTUONG_SMTP_* env vars.
    pub require_email_verification: bool,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let cookie_secure = std::env::var("COTUONG_COOKIE_SECURE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let public_base_url = std::env::var("COTUONG_PUBLIC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        let require_email_verification = std::env::var("COTUONG_EMAIL_VERIFY")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        Self {
            cookie_secure,
            public_base_url,
            require_email_verification,
        }
    }
}

impl FromRef<AppState> for Arc<Hub> {
    fn from_ref(s: &AppState) -> Self {
        s.hub.clone()
    }
}

impl FromRef<AppState> for Arc<dyn Storage> {
    fn from_ref(s: &AppState) -> Self {
        s.storage.clone()
    }
}

impl FromRef<AppState> for Arc<AuthConfig> {
    fn from_ref(s: &AppState) -> Self {
        s.cfg.clone()
    }
}
