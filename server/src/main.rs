use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod hub;
mod proto;
mod room;

use hub::Hub;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "cotuong_server=info,tower_http=warn".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let hub = Arc::new(Hub::new());

    // Serve the static frontend. By default, look at ../web relative to the
    // workspace root; override with COTUONG_WEB.
    let web_dir = std::env::var("COTUONG_WEB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("web"));

    if !web_dir.exists() {
        tracing::warn!(
            "web dir {:?} does not exist; the /ws endpoint will still work but / will 404",
            web_dir
        );
    }

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(|| async { "ok" }))
        .fallback_service(ServeDir::new(&web_dir))
        .with_state(hub);

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

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(hub): State<Arc<Hub>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| hub::handle_session(socket, hub))
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
