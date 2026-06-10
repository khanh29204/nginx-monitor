use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

// Combined Log Format:
// $remote_addr - $remote_user [$time_local] "$request" $status $body_bytes_sent "$http_referer" "$http_user_agent"
static LOG_REGEX: OnceLock<Regex> = OnceLock::new();

fn log_regex() -> &'static Regex {
    LOG_REGEX.get_or_init(|| {
        Regex::new(r#"^(\S+) - (\S+) \[([^\]]+)\] "([^"]*)" (\d{3}) (\d+) "([^"]*)" "([^"]*)""#)
            .unwrap()
    })
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub ip: String,
    pub timestamp: DateTime<Utc>,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub bytes: u64,
    pub user_agent: String,
    /// Estimated heap size of this entry in bytes
    pub heap_size: usize,
}

impl LogEntry {
    pub fn estimate_size(&self) -> usize {
        // Fixed struct overhead + string heap allocations
        std::mem::size_of::<LogEntry>()
            + self.ip.len()
            + self.method.len()
            + self.url.len()
            + self.user_agent.len()
    }
}

pub fn parse_line(line: &str) -> Option<LogEntry> {
    let re = log_regex();
    let caps = re.captures(line)?;

    let ip = caps[1].to_string();
    let time_str = &caps[3];
    let request = &caps[4];
    let status: u16 = caps[5].parse().ok()?;
    let bytes: u64 = caps[6].parse().unwrap_or(0);
    let user_agent = caps[8].to_string();

    // Parse time: "10/Jun/2026:00:00:02 +0700"
    let timestamp = chrono::DateTime::parse_from_str(time_str, "%d/%b/%Y:%H:%M:%S %z")
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    // Parse request: "GET /path HTTP/1.1"
    let mut parts = request.splitn(3, ' ');
    let method = parts.next().unwrap_or("-").to_string();
    let url = parts.next().unwrap_or("-").to_string();

    let mut entry = LogEntry {
        ip,
        timestamp,
        method,
        url,
        status,
        bytes,
        user_agent,
        heap_size: 0,
    };
    entry.heap_size = entry.estimate_size();

    Some(entry)
}

// ─── In-memory store ──────────────────────────────────────────────────────────

pub struct LogStore {
    entries: VecDeque<LogEntry>,
    total_bytes: usize,
    cap_bytes: usize,
}

impl LogStore {
    pub fn new(cap_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            total_bytes: 0,
            cap_bytes,
        }
    }

    /// Push entry, evict oldest if over cap (FIFO)
    pub fn push(&mut self, entry: LogEntry) {
        let size = entry.heap_size;
        self.entries.push_back(entry);
        self.total_bytes += size;

        while self.total_bytes > self.cap_bytes {
            if let Some(old) = self.entries.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(old.heap_size);
            } else {
                break;
            }
        }
    }

    pub fn snapshot(&self) -> AccessLogSnapshot {
        let total = self.entries.len() as u64;

        let mut status_map: HashMap<u16, u64> = HashMap::new();
        let mut ip_map: HashMap<String, u64> = HashMap::new();
        let mut url_map: HashMap<String, u64> = HashMap::new();
        let mut total_bytes: u64 = 0;

        // requests per minute: count entries in last 60s
        let now = Utc::now();
        let mut rpm: u64 = 0;

        for e in &self.entries {
            *status_map.entry(e.status).or_insert(0) += 1;
            *ip_map.entry(e.ip.clone()).or_insert(0) += 1;

            // Strip query string for URL grouping
            let path = e.url.split('?').next().unwrap_or(&e.url).to_string();
            *url_map.entry(path).or_insert(0) += 1;

            total_bytes += e.bytes;

            let age = now.signed_duration_since(e.timestamp);
            if age.num_seconds() <= 60 {
                rpm += 1;
            }
        }

        // top 10 IPs
        let mut top_ips: Vec<IpCount> = ip_map
            .into_iter()
            .map(|(ip, count)| IpCount { ip, count })
            .collect();
        top_ips.sort_by(|a, b| b.count.cmp(&a.count));
        top_ips.truncate(10);

        // top 10 URLs
        let mut top_urls: Vec<UrlCount> = url_map
            .into_iter()
            .map(|(url, count)| UrlCount { url, count })
            .collect();
        top_urls.sort_by(|a, b| b.count.cmp(&a.count));
        top_urls.truncate(10);

        // status codes as string keys for JSON
        let status_codes: HashMap<String, u64> = status_map
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        AccessLogSnapshot {
            total_requests: total,
            requests_last_minute: rpm,
            status_codes,
            top_ips,
            top_urls,
            total_bandwidth_bytes: total_bytes,
            memory_used_bytes: self.total_bytes as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpCount {
    pub ip: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlCount {
    pub url: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogSnapshot {
    pub total_requests: u64,
    pub requests_last_minute: u64,
    pub status_codes: HashMap<String, u64>,
    pub top_ips: Vec<IpCount>,
    pub top_urls: Vec<UrlCount>,
    pub total_bandwidth_bytes: u64,
    pub memory_used_bytes: u64,
}
