//! Anomaly detection — statistical spike identification over sliding windows.
//!
//! Uses a simple rolling mean + standard-deviation approach:
//!   - **Error-rate spike**: per-minute error counts exceed mean + N·σ
//!   - **Latency spike**: response_time entries that exceed mean + M·σ
//!
//! The detector is intentionally stateless per call so the TUI can call it
//! after every new batch without maintaining complex shared state.

use super::parser::{LogEntry, LogLevel};

// ─── AnomalyEvent ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AnomalyKind {
    ErrorSpike,
    LatencySpike,
}

#[derive(Debug, Clone)]
pub struct AnomalyEvent {
    pub kind:        AnomalyKind,
    pub description: String,
    pub entry_idx:   usize,
}

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnomalyConfig {
    /// σ multiplier for error-rate spikes (default 3.0)
    pub error_sigma:   f64,
    /// σ multiplier for latency spikes (default 2.5)
    pub latency_sigma: f64,
    /// Minimum data points before anomaly detection activates (default 10)
    pub min_samples:   usize,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        AnomalyConfig {
            error_sigma:   3.0,
            latency_sigma: 2.5,
            min_samples:   10,
        }
    }
}

// ─── Core statistics ─────────────────────────────────────────────────────────

fn mean_stddev(values: &[f64]) -> Option<(f64, f64)> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    Some((mean, variance.sqrt()))
}

// ─── Latency anomaly ──────────────────────────────────────────────────────────

/// Tag entries whose response_time is an outlier (mean + sigma * stddev).
/// Returns a list of `AnomalyEvent` for each such entry.
pub fn detect_latency_anomalies(
    entries: &mut Vec<LogEntry>,
    cfg:     &AnomalyConfig,
) -> Vec<AnomalyEvent> {
    let rts: Vec<f64> = entries.iter()
        .filter_map(|e| e.response_time)
        .collect();

    if rts.len() < cfg.min_samples {
        return vec![];
    }

    let Some((mean, stddev)) = mean_stddev(&rts) else {
        return vec![];
    };

    let threshold = mean + cfg.latency_sigma * stddev;
    let mut events = Vec::new();

    for (i, e) in entries.iter_mut().enumerate() {
        if let Some(rt) = e.response_time {
            if rt > threshold {
                e.anomaly = true;
                events.push(AnomalyEvent {
                    kind: AnomalyKind::LatencySpike,
                    description: format!(
                        "Latency spike: {:.0}ms (threshold {:.0}ms, mean {:.0}ms)",
                        rt, threshold, mean
                    ),
                    entry_idx: i,
                });
            }
        }
    }
    events
}

// ─── Error-rate anomaly ───────────────────────────────────────────────────────

/// Bucket errors by 60-second windows; flag buckets that are error-rate spikes.
pub fn detect_error_rate_anomalies(
    entries: &mut Vec<LogEntry>,
    cfg:     &AnomalyConfig,
) -> Vec<AnomalyEvent> {
    // Collect (bucket_key, index) pairs. bucket_key = floor(epoch_seconds / 60).
    #[cfg(feature = "logs")]
    {
        use std::collections::BTreeMap;

        // Gather entries with timestamps into minute-buckets.
        let mut buckets: BTreeMap<i64, usize> = BTreeMap::new();
        let mut bucket_first_entry: BTreeMap<i64, usize> = BTreeMap::new();

        for (i, e) in entries.iter().enumerate() {
            if e.level == LogLevel::Error {
                if let Some(ts) = e.timestamp {
                    let bucket = ts.timestamp() / 60;
                    *buckets.entry(bucket).or_insert(0) += 1;
                    bucket_first_entry.entry(bucket).or_insert(i);
                }
            }
        }

        if buckets.len() < cfg.min_samples.min(3) {
            return vec![];
        }

        let counts: Vec<f64> = buckets.values().map(|&c| c as f64).collect();
        let Some((mean, stddev)) = mean_stddev(&counts) else {
            return vec![];
        };

        let threshold = mean + cfg.error_sigma * stddev;
        let mut events = Vec::new();

        for (&bucket, &count) in &buckets {
            if count as f64 > threshold {
                // Mark the first entry in this bucket as anomaly.
                if let Some(&first_idx) = bucket_first_entry.get(&bucket) {
                    entries[first_idx].anomaly = true;
                    events.push(AnomalyEvent {
                        kind: AnomalyKind::ErrorSpike,
                        description: format!(
                            "Error spike: {} errors/min (threshold {:.1}, mean {:.1})",
                            count, threshold, mean
                        ),
                        entry_idx: first_idx,
                    });
                }
            }
        }
        return events;
    }

    #[cfg(not(feature = "logs"))]
    vec![]
}

/// Run all detectors and mutate `entries` in-place.
pub fn run_all(entries: &mut Vec<LogEntry>, cfg: &AnomalyConfig) -> Vec<AnomalyEvent> {
    let mut events = detect_latency_anomalies(entries, cfg);
    events.extend(detect_error_rate_anomalies(entries, cfg));
    events
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser::parse_line;

    #[test]
    fn latency_spike_detected() {
        let mut entries: Vec<LogEntry> = (0..20)
            .map(|i| {
                let rt = if i == 19 { 9999.0 } else { 100.0 };
                let raw = format!(
                    r#"{{"level":"INFO","message":"req","response_time":{}}}"#,
                    rt
                );
                let mut e = parse_line(&raw, i as u64);
                e.response_time = Some(rt);
                e
            })
            .collect();

        let cfg = AnomalyConfig { min_samples: 5, ..Default::default() };
        let events = detect_latency_anomalies(&mut entries, &cfg);
        assert!(!events.is_empty(), "spike should be detected");
        assert!(entries[19].anomaly);
    }

    #[test]
    fn no_anomaly_when_uniform() {
        let mut entries: Vec<LogEntry> = (0..20)
            .map(|i| {
                let raw = r#"{"level":"INFO","message":"req","response_time":100.0}"#;
                parse_line(raw, i as u64)
            })
            .collect();
        let cfg = AnomalyConfig::default();
        let events = detect_latency_anomalies(&mut entries, &cfg);
        assert!(events.is_empty());
    }

    #[test]
    fn min_samples_guard() {
        let mut entries: Vec<LogEntry> = (0..5)
            .map(|i| {
                let raw = r#"{"level":"INFO","message":"req","response_time":9999.0}"#;
                parse_line(raw, i as u64)
            })
            .collect();
        let cfg = AnomalyConfig { min_samples: 20, ..Default::default() };
        let events = detect_latency_anomalies(&mut entries, &cfg);
        assert!(events.is_empty(), "should skip with too few samples");
    }
}
