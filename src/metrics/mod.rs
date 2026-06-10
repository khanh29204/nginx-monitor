pub mod access_log;
pub mod stub_status;
pub mod system;

use crate::config::Config;
use access_log::{AccessLogSnapshot, LogStore};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stub_status::StubStatus;
use system::{SystemCollector, SystemMetrics};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: String,
    pub stub_status: StubStatus,
    pub access_log: AccessLogSnapshot,
    pub system: SystemMetrics,
}

pub type SharedStore = Arc<RwLock<LogStore>>;
pub type MetricsTx = broadcast::Sender<MetricsSnapshot>;

/// Spawn background tasks:
/// 1. Tail access.log → push to LogStore
/// 2. Poll stub_status + system → broadcast MetricsSnapshot
pub async fn start_collectors(
    config: Arc<Config>,
    store: SharedStore,
    tx: MetricsTx,
) {
    // Task 1: tail access.log
    let store_clone = store.clone();
    let log_path = config.access_log_path.clone();
    tokio::spawn(async move {
        tail_log(log_path, store_clone).await;
    });

    // Task 2: poll metrics + broadcast
    let config_clone = config.clone();
    tokio::spawn(async move {
        poll_and_broadcast(config_clone, store, tx).await;
    });
}

async fn tail_log(path: String, store: SharedStore) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    loop {
        match tokio::fs::File::open(&path).await {
            Err(e) => {
                error!("Cannot open log file {}: {}", path, e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            Ok(file) => {
                info!("Tailing log file: {}", path);
                let mut reader = BufReader::new(file);
                let mut line = String::new();

                // Seek to end to only read new lines
                use tokio::io::AsyncSeekExt;
                if let Err(e) = reader.get_mut().seek(std::io::SeekFrom::End(0)).await {
                    error!("Seek error: {}", e);
                }

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => {
                            // EOF → wait for new data
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                        Ok(_) => {
                            let trimmed = line.trim_end();
                            if let Some(entry) = access_log::parse_line(trimmed) {
                                let mut store = store.write().await;
                                store.push(entry);
                            }
                        }
                        Err(e) => {
                            warn!("Read error: {}, reopening...", e);
                            break; // reopen file (log rotation)
                        }
                    }
                }
            }
        }
    }
}

async fn poll_and_broadcast(config: Arc<Config>, store: SharedStore, tx: MetricsTx) {
    // Use 1s base tick; clients control their own interval via SSE
    let mut ticker = interval(Duration::from_secs(1));
    let mut sys_collector = SystemCollector::new();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    loop {
        ticker.tick().await;

        let stub = fetch_stub_status(&http, &config.stub_status_url).await;
        let system = sys_collector.collect();
        let access_log = {
            let store = store.read().await;
            store.snapshot()
        };

        let snapshot = MetricsSnapshot {
            timestamp: Utc::now().to_rfc3339(),
            stub_status: stub,
            access_log,
            system,
        };

        // Ignore error if no subscribers
        let _ = tx.send(snapshot);
    }
}

async fn fetch_stub_status(client: &reqwest::Client, url: &str) -> StubStatus {
    match client.get(url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => stub_status::parse(&text).unwrap_or_default(),
            Err(e) => {
                warn!("stub_status read error: {}", e);
                StubStatus::default()
            }
        },
        Err(e) => {
            warn!("stub_status fetch error: {}", e);
            StubStatus::default()
        }
    }
}
