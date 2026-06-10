mod config;
mod handler;
mod metrics;

use axum::{
    routing::get,
    Router,
};
use config::Config;
use metrics::{access_log::LogStore, start_collectors, MetricsTx, SharedStore};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub metrics_tx: MetricsTx,
    pub http: Client,
}

#[tokio::main]
async fn main() {
    // Load .env if present
    let _ = dotenvy::dotenv();

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nginx_monitor=info".into()),
        )
        .init();

    let config = Arc::new(Config::from_env());
    info!("Starting nginx-monitor on port {}", config.port);

    // Shared log store (FIFO, 70MB cap)
    let store: SharedStore = Arc::new(RwLock::new(LogStore::new(config.memory_cap_bytes)));

    // Broadcast channel: capacity 64 (latest metrics, clients can be slow)
    let (tx, _) = broadcast::channel::<metrics::MetricsSnapshot>(64);

    // Start background collectors
    start_collectors(config.clone(), store.clone(), tx.clone()).await;

    let state = AppState {
        config: config.clone(),
        metrics_tx: tx,
        http: Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap(),
    };

    let app = Router::new()
        .route("/stream", get(handler::sse::sse_handler))
        .route("/health", get(health_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!("Listening on {}", addr);

    axum::serve(listener, app).await.unwrap();
}

async fn health_handler() -> &'static str {
    "ok"
}
