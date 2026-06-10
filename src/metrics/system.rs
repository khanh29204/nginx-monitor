use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_percent: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub ram_percent: f32,
}

pub struct SystemCollector {
    sys: System,
}

impl SystemCollector {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    pub fn collect(&mut self) -> SystemMetrics {
        self.sys.refresh_cpu();
        self.sys.refresh_memory();

        let cpu_percent = self.sys.global_cpu_info().cpu_usage();
        let ram_used = self.sys.used_memory(); // bytes
        let ram_total = self.sys.total_memory(); // bytes

        let ram_used_mb = ram_used / 1024 / 1024;
        let ram_total_mb = ram_total / 1024 / 1024;
        let ram_percent = if ram_total > 0 {
            (ram_used as f32 / ram_total as f32) * 100.0
        } else {
            0.0
        };

        SystemMetrics {
            cpu_percent,
            ram_used_mb,
            ram_total_mb,
            ram_percent,
        }
    }
}
