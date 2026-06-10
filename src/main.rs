mod config;
mod handler;
mod metrics;

use axum::{http::HeaderValue, routing::get, Router};
use config::Config;
use metrics::{access_log::LogStore, start_collectors, MetricsTx, SharedStore};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub metrics_tx: MetricsTx,
    pub http: Client,
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nginx_monitor=info".into()),
        )
        .init();

    let config = Arc::new(Config::from_env());
    info!("Starting nginx-monitor on port {}", config.port);
    info!("CORS origins: {:?}", config.cors_origins);

    let store: SharedStore = Arc::new(RwLock::new(LogStore::new(config.memory_cap_bytes)));
    let (tx, _) = broadcast::channel::<metrics::MetricsSnapshot>(64);
    start_collectors(config.clone(), store.clone(), tx.clone()).await;

    let state = AppState {
        config: config.clone(),
        metrics_tx: tx,
        http: Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap(),
    };

    let cors = build_cors(&config.cors_origins);

    let app = Router::new()
        .route("/stream", get(handler::sse::sse_handler))
        .route("/health", get(health_handler))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!("Listening on {}", addr);

    axum::serve(listener, app).await.unwrap();
}

fn build_cors(origins: &[String]) -> CorsLayer {
    if origins.iter().any(|o| o == "*") {
        CorsLayer::permissive()
    } else {
        let parsed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(parsed))
            .allow_headers(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
    }
}

async fn health_handler() -> &'static str {
    "ok"
}
