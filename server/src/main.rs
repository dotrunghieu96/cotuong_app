use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod api;
mod auth;
mod db;
mod hub;
mod proto;
mod room;
mod state;

use db::Storage;
use hub::Hub;
use state::{AppState, AuthConfig};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "cotuong_server=info,tower_http=warn".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let auth_cfg = Arc::new(AuthConfig::from_env());

    // Pre-flight: refuse to start with verification enabled but no email path.
    if auth_cfg.require_email_verification {
        #[cfg(not(feature = "email"))]
        {
            tracing::error!(
                "COTUONG_EMAIL_VERIFY is set but the binary was built without the `email` \
                 feature. Rebuild with `--features email` or unset the variable."
            );
            std::process::exit(1);
        }
        #[cfg(feature = "email")]
        {
            if auth::email::SmtpConfig::from_env().is_none() {
                tracing::error!(
                    "COTUONG_EMAIL_VERIFY is set but COTUONG_SMTP_* vars are missing. \
                     Either configure SMTP or unset COTUONG_EMAIL_VERIFY."
                );
                std::process::exit(1);
            }
        }
    }

    let db_url = std::env::var("COTUONG_DB_URL").unwrap_or_else(|_| "sqlite:cotuong.db".into());
    let storage: Arc<dyn Storage> = match db::connect(&db_url).await {
        Ok(s) => {
            tracing::info!("storage connected: {}", redact_url(&db_url));
            s
        }
        Err(e) => {
            tracing::error!("storage connection failed for {}: {e}", redact_url(&db_url));
            std::process::exit(1);
        }
    };

    #[cfg(feature = "r2-backup")]
    spawn_backup_if_configured(&db_url);

    let hub = Arc::new(Hub::new(storage.clone()));

    let app_state = AppState {
        hub: hub.clone(),
        storage: storage.clone(),
        cfg: auth_cfg.clone(),
    };

    let web_dir = std::env::var("COTUONG_WEB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("web"));

    if !web_dir.exists() {
        tracing::warn!(
            "web dir {:?} does not exist; the /ws endpoint will still work but / will 404",
            web_dir
        );
    }

    let api_routes = Router::new()
        .route("/games", get(api::list_games))
        .route("/games/:id", get(api::get_game))
        .route("/games/:id/moves", get(api::get_moves));

    let auth_routes = build_auth_routes();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api_routes)
        .nest("/auth", auth_routes)
        .fallback_service(ServeDir::new(&web_dir))
        .with_state(app_state);

    let addr: SocketAddr = std::env::var("COTUONG_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8000".to_string())
        .parse()
        .expect("COTUONG_ADDR must be a valid socket address");

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(
        "cotuong_server listening on http://{} (web dir: {})",
        addr,
        web_dir.display()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

fn build_auth_routes() -> Router<AppState> {
    let mut r = Router::new()
        .route("/signup", post(auth::handlers::signup))
        .route("/login", post(auth::handlers::login))
        .route("/logout", post(auth::handlers::logout))
        .route("/me", get(auth::handlers::me))
        .route("/verify/:token", get(auth::handlers::verify_email))
        .route(
            "/password-reset/request",
            post(auth::handlers::password_reset_request),
        )
        .route(
            "/password-reset/confirm",
            post(auth::handlers::password_reset_confirm),
        );

    #[cfg(feature = "oauth")]
    {
        r = r
            .route("/google/login", get(auth::oauth::login))
            .route("/google/callback", get(auth::oauth::callback));
    }
    #[cfg(not(feature = "oauth"))]
    {
        r = r
            .route("/google/login", get(auth_oauth_disabled))
            .route("/google/callback", get(auth_oauth_disabled));
    }
    r
}

#[cfg(not(feature = "oauth"))]
async fn auth_oauth_disabled() -> impl IntoResponse {
    (
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Google OAuth is not enabled (rebuild with --features oauth)",
    )
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = auth::session::user_from_headers(&state.storage, &headers).await;
    ws.on_upgrade(move |socket| hub::handle_session(socket, state.hub, user))
}

#[cfg(feature = "r2-backup")]
fn spawn_backup_if_configured(db_url: &str) {
    if !db_url.starts_with("sqlite:") {
        return;
    }
    let Some(cfg) = db::backup::BackupConfig::from_env() else {
        return;
    };
    let url = db_url.to_string();
    tokio::spawn(async move {
        match db::sqlite::SqliteStorage::connect(&url).await {
            Ok(s) => db::backup::spawn(Arc::new(s), cfg),
            Err(e) => tracing::warn!("R2 backup connect failed: {e}"),
        }
    });
}

fn redact_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("postgres://").or_else(|| url.strip_prefix("postgresql://")) {
        if let Some(at) = rest.find('@') {
            let scheme = if url.starts_with("postgresql://") { "postgresql" } else { "postgres" };
            return format!("{}://***@{}", scheme, &rest[at + 1..]);
        }
    }
    url.to_string()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install ctrl-c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received");
}
