//! HiTech-OS Autonomic Nervous System (`os-nerve`).
//!
//! Zero-overhead kernel telemetry sensor reading directly from `/proc/pressure/*` (PSI),
//! `/proc/meminfo`, `/proc/loadavg`, and `/proc/[pid]/statm`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PsiMetric {
    pub avg10: f32,
    pub avg60: f32,
    pub total_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopProcess {
    pub pid: u32,
    pub comm: String,
    pub rss_mb: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NervePulse {
    pub mem_total_mb: f32,
    pub mem_available_mb: f32,
    pub mem_used_percent: f32,
    pub swap_used_mb: f32,
    pub load_1m: f32,
    pub load_5m: f32,
    pub cpu_psi: PsiMetric,
    pub mem_psi: PsiMetric,
    pub io_psi: PsiMetric,
    pub top_memory_hog: Option<TopProcess>,
    pub anomalies: Vec<String>,
    pub healthy: bool,
}

impl NervePulse {
    pub fn sense() -> Self {
        let (mem_total_mb, mem_available_mb, mem_used_percent, swap_used_mb) = read_meminfo();
        let (load_1m, load_5m) = read_loadavg();
        let cpu_psi = read_psi("/proc/pressure/cpu");
        let mem_psi = read_psi("/proc/pressure/memory");
        let io_psi = read_psi("/proc/pressure/io");
        let top_memory_hog = find_top_memory_hog();

        let mut anomalies = Vec::new();

        if mem_used_percent >= 88.0 {
            if let Some(ref hog) = top_memory_hog {
                anomalies.push(format!(
                    "critical_memory_pressure: {:.1}% RAM used (top consumer: {} PID {} using {:.1} MB)",
                    mem_used_percent, hog.comm, hog.pid, hog.rss_mb
                ));
            } else {
                anomalies.push(format!(
                    "critical_memory_pressure: {:.1}% RAM used",
                    mem_used_percent
                ));
            }
        }

        if mem_psi.avg10 >= 10.0 {
            anomalies.push(format!(
                "memory_thrashing_psi: stall avg10={:.2}% avg60={:.2}%",
                mem_psi.avg10, mem_psi.avg60
            ));
        }

        if cpu_psi.avg10 >= 25.0 {
            anomalies.push(format!(
                "cpu_saturation_psi: stall avg10={:.2}% (load_1m={:.2})",
                cpu_psi.avg10, load_1m
            ));
        }

        if io_psi.avg10 >= 20.0 {
            anomalies.push(format!(
                "io_bottleneck_psi: stall avg10={:.2}%",
                io_psi.avg10
            ));
        }

        let healthy = anomalies.is_empty();

        Self {
            mem_total_mb,
            mem_available_mb,
            mem_used_percent,
            swap_used_mb,
            load_1m,
            load_5m,
            cpu_psi,
            mem_psi,
            io_psi,
            top_memory_hog,
            anomalies,
            healthy,
        }
    }
}

fn read_meminfo() -> (f32, f32, f32, f32) {
    let Ok(raw) = fs::read_to_string("/proc/meminfo") else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    let mut swap_total_kb = 0u64;
    let mut swap_free_kb = 0u64;

    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let Some(val) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
            continue;
        };
        match key {
            "MemTotal:" => total_kb = val,
            "MemAvailable:" => avail_kb = val,
            "SwapTotal:" => swap_total_kb = val,
            "SwapFree:" => swap_free_kb = val,
            _ => {}
        }
    }

    if total_kb == 0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let total_mb = total_kb as f32 / 1024.0;
    let avail_mb = avail_kb as f32 / 1024.0;
    let used_pct = ((total_kb.saturating_sub(avail_kb)) as f32 / total_kb as f32) * 100.0;
    let swap_used_mb = (swap_total_kb.saturating_sub(swap_free_kb)) as f32 / 1024.0;
    (
        (total_mb * 10.0).round() / 10.0,
        (avail_mb * 10.0).round() / 10.0,
        (used_pct * 10.0).round() / 10.0,
        (swap_used_mb * 10.0).round() / 10.0,
    )
}

fn read_loadavg() -> (f32, f32) {
    let Ok(raw) = fs::read_to_string("/proc/loadavg") else {
        return (0.0, 0.0);
    };
    let mut parts = raw.split_whitespace();
    let l1 = parts
        .next()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let l5 = parts
        .next()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    (l1, l5)
}

fn read_psi(path: &str) -> PsiMetric {
    let Ok(raw) = fs::read_to_string(path) else {
        return PsiMetric::default();
    };
    let mut metric = PsiMetric::default();
    for line in raw.lines() {
        if !line.starts_with("some ") {
            continue;
        }
        for token in line.split_whitespace().skip(1) {
            if let Some((k, v)) = token.split_once('=') {
                match k {
                    "avg10" => metric.avg10 = v.parse().unwrap_or(0.0),
                    "avg60" => metric.avg60 = v.parse().unwrap_or(0.0),
                    "total" => metric.total_us = v.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
    }
    metric
}

fn find_top_memory_hog() -> Option<TopProcess> {
    let entries = fs::read_dir(Path::new("/proc")).ok()?;
    let page_size_kb = 4u64;
    let mut best: Option<TopProcess> = None;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };

        let statm_path = format!("/proc/{pid}/statm");
        let Ok(statm) = fs::read_to_string(&statm_path) else {
            continue;
        };
        let resident_pages = statm
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let rss_mb = (resident_pages * page_size_kb) as f32 / 1024.0;

        if rss_mb > best.as_ref().map(|b| b.rss_mb).unwrap_or(0.0) {
            let comm = fs::read_to_string(format!("/proc/{pid}/comm"))
                .unwrap_or_else(|_| "?".to_string())
                .trim()
                .to_string();
            best = Some(TopProcess {
                pid,
                comm,
                rss_mb: (rss_mb * 10.0).round() / 10.0,
            });
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerve_pulse_senses_host_metrics_safely() {
        let pulse = NervePulse::sense();
        assert!(pulse.mem_total_mb >= 0.0);
        assert!(pulse.mem_used_percent >= 0.0 && pulse.mem_used_percent <= 100.0);
    }
}
