//! Versioned JSON context sent to the model.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bump when the shape changes incompatibly.
pub const CONTEXT_VERSION: u32 = 1;

/// Full server context snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerContext {
    pub context_version: u32,
    pub collected_at_unix_ms: u128,
    pub hostname: String,
    pub cwd: String,
    pub metrics: Option<MetricsSnapshot>,
    /// Summarized log file contents.
    pub logs: Vec<LogSummary>,
    /// `.env`-style keys (values redacted unless opted in).
    pub dotenv: Vec<DotenvLine>,
    /// Snippets from nginx / docker / systemd / other configs.
    pub configs: Vec<ConfigSnippet>,
    /// Human-readable notes (permissions, truncation, etc.).
    pub notes: Vec<String>,
}

impl ServerContext {
    pub fn empty(hostname: String, cwd: String, notes: Vec<String>) -> Self {
        Self {
            context_version: CONTEXT_VERSION,
            collected_at_unix_ms: now_ms(),
            hostname,
            cwd,
            metrics: None,
            logs: vec![],
            dotenv: vec![],
            configs: vec![],
            notes,
        }
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub cpus: usize,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub load_average: Option<(f64, f64, f64)>,
    pub disks: Vec<DiskMetric>,
    pub top_processes: Vec<ProcessMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetric {
    pub mount: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetric {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
    pub cpu_usage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSummary {
    pub path: String,
    pub truncated: bool,
    pub line_count_in_sample: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotenvLine {
    pub path: String,
    pub key: Option<String>,
    pub raw_redacted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnippet {
    pub kind: String,
    pub path: String,
    pub truncated: bool,
    pub content: String,
}

/// Tunables for collectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectConfig {
    pub log_roots: Vec<PathBuf>,
    pub nginx_paths: Vec<PathBuf>,
    pub docker_paths: Vec<PathBuf>,
    pub systemd_paths: Vec<PathBuf>,
    pub env_search_paths: Vec<PathBuf>,
    pub max_file_bytes: usize,
    pub max_log_files: usize,
    pub max_log_total_bytes: usize,
    pub max_lines_per_log: usize,
    pub top_processes: usize,
    pub include_secrets: bool,
    pub concurrency: usize,
    pub cache_ttl_secs: u64,
}

impl Default for CollectConfig {
    fn default() -> Self {
        let mut log_roots = vec![PathBuf::from("/var/log")];
        if let Ok(cwd) = std::env::current_dir() {
            log_roots.push(cwd.join("logs"));
        }
        Self {
            log_roots,
            nginx_paths: vec![
                PathBuf::from("/etc/nginx/nginx.conf"),
                PathBuf::from("/etc/nginx/sites-available"),
                PathBuf::from("/etc/nginx/sites-enabled"),
            ],
            docker_paths: vec![
                PathBuf::from("/etc/docker/daemon.json"),
                dirs::home_dir()
                    .map(|h| h.join(".docker/config.json"))
                    .unwrap_or_else(|| PathBuf::from(".docker/config.json")),
            ],
            systemd_paths: vec![
                PathBuf::from("/etc/systemd/system"),
                PathBuf::from("/lib/systemd/system"),
            ],
            env_search_paths: vec![],
            max_file_bytes: 256 * 1024,
            max_log_files: 80,
            max_log_total_bytes: 600 * 1024,
            max_lines_per_log: 120,
            top_processes: 25,
            include_secrets: false,
            concurrency: 32,
            cache_ttl_secs: 90,
        }
    }
}

impl CollectConfig {
    pub fn with_cwd(cwd: PathBuf) -> Self {
        let mut c = Self::default();
        c.env_search_paths.push(cwd);
        c
    }
}
