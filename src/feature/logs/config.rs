//! Config loading for the Smart Log Explorer.
//!
//! Reads `~/.config/termedit/logs.toml` (falls back to compiled-in defaults).

use serde::Deserialize;
use std::collections::HashMap;

use super::anomaly::AnomalyConfig;

// ─── Custom log pattern ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CustomPattern {
    /// Display name for this format.
    pub name: String,
    /// Regex pattern. Named capture groups map to log fields.
    pub pattern: String,
    /// Maps log field names (level, service, message, response_time, timestamp)
    /// to regex capture-group names.
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

// ─── LogsConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct LogsConfig {
    /// σ multiplier for error-rate anomaly detection (default 3.0).
    #[serde(default = "default_error_sigma")]
    pub anomaly_error_sigma: f64,

    /// σ multiplier for latency anomaly detection (default 2.5).
    #[serde(default = "default_latency_sigma")]
    pub anomaly_latency_sigma: f64,

    /// Ring-buffer capacity for the log viewer (default 50_000 lines).
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,

    /// Minimum samples before anomaly detection fires (default 10).
    #[serde(default = "default_min_anomaly_samples")]
    pub min_anomaly_samples: usize,

    /// Polling interval in ms when `notify` is not available (default 80).
    #[serde(default = "default_tail_poll_ms")]
    pub tail_poll_ms: u64,

    /// User-defined custom log-format patterns.
    #[serde(default)]
    pub custom_patterns: Vec<CustomPattern>,
}

fn default_error_sigma()        -> f64   { 3.0 }
fn default_latency_sigma()      -> f64   { 2.5 }
fn default_max_lines()          -> usize { 50_000 }
fn default_min_anomaly_samples()-> usize { 10 }
fn default_tail_poll_ms()       -> u64   { 80 }

impl Default for LogsConfig {
    fn default() -> Self {
        LogsConfig {
            anomaly_error_sigma:   default_error_sigma(),
            anomaly_latency_sigma: default_latency_sigma(),
            max_lines:             default_max_lines(),
            min_anomaly_samples:   default_min_anomaly_samples(),
            tail_poll_ms:          default_tail_poll_ms(),
            custom_patterns:       vec![],
        }
    }
}

impl LogsConfig {
    /// Load from `~/.config/termedit/logs.toml`, falling back to defaults on error.
    pub fn load() -> Self {
        if let Some(config_dir) = dirs::config_dir() {
            let path = config_dir.join("termedit").join("logs.toml");
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(cfg) = toml::from_str::<LogsConfig>(&content) {
                        return cfg;
                    }
                }
            }
        }
        LogsConfig::default()
    }

    /// Convert to the anomaly-detector config.
    pub fn anomaly_config(&self) -> AnomalyConfig {
        AnomalyConfig {
            error_sigma:   self.anomaly_error_sigma,
            latency_sigma: self.anomaly_latency_sigma,
            min_samples:   self.min_anomaly_samples,
        }
    }
}
