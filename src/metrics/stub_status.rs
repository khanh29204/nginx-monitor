use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StubStatus {
    pub active_connections: u64,
    pub accepts: u64,
    pub handled: u64,
    pub requests: u64,
    pub reading: u64,
    pub writing: u64,
    pub waiting: u64,
}

pub fn parse(raw: &str) -> Option<StubStatus> {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.len() < 4 {
        return None;
    }

    // Line 0: "Active connections: 43"
    let active_connections = lines[0]
        .split_whitespace()
        .last()?
        .parse()
        .ok()?;

    // Line 2: " 1000 1000 5000"
    let counters: Vec<u64> = lines[2]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();

    if counters.len() < 3 {
        return None;
    }

    // Line 3: "Reading: 0 Writing: 5 Waiting: 38"
    let state: Vec<u64> = lines[3]
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();

    if state.len() < 3 {
        return None;
    }

    Some(StubStatus {
        active_connections,
        accepts: counters[0],
        handled: counters[1],
        requests: counters[2],
        reading: state[0],
        writing: state[1],
        waiting: state[2],
    })
}
