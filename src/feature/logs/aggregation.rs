//! Aggregation engine — group-by counts and time-based windowing.

use std::collections::BTreeMap;
use super::parser::{LogEntry, LogLevel};
use super::query::GroupField;

// ─── GroupBy ─────────────────────────────────────────────────────────────────

/// Aggregate entries by a field, returning a sorted map of (key → count).
pub fn group_by<'a>(
    field: &GroupField,
    entries: impl Iterator<Item = &'a LogEntry>,
) -> BTreeMap<String, usize> {
    let mut map: BTreeMap<String, usize> = BTreeMap::new();
    for e in entries {
        let key = match field {
            GroupField::Level => e.level.label().trim().to_string(),
            GroupField::Service => e.service.clone().unwrap_or_else(|| "(unknown)".to_string()),
            GroupField::Status => {
                // Extract a 3-digit HTTP status from raw if present
                extract_status(&e.raw)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "(none)".to_string())
            }
        };
        *map.entry(key).or_insert(0) += 1;
    }
    map
}

fn extract_status(raw: &str) -> Option<u16> {
    // find a 3-digit number that looks like an HTTP status (1xx–5xx)
    let mut chars = raw.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_ascii_digit() {
            let end = raw[i..].find(|c: char| !c.is_ascii_digit()).map(|n| i + n).unwrap_or(raw.len());
            let tok = &raw[i..end];
            if tok.len() == 3 {
                if let Ok(n) = tok.parse::<u16>() {
                    if (100..=599).contains(&n) {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}

// ─── Summary stats ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct LogStats {
    pub total:   usize,
    pub errors:  usize,
    pub warns:   usize,
    pub anomalies: usize,
    pub avg_response_time: Option<f64>,
    pub p95_response_time: Option<f64>,
}

pub fn compute_stats(entries: &[LogEntry]) -> LogStats {
    let mut stats = LogStats {
        total: entries.len(),
        ..Default::default()
    };
    let mut rts: Vec<f64> = Vec::new();

    for e in entries {
        match e.level {
            LogLevel::Error => stats.errors += 1,
            LogLevel::Warn  => stats.warns  += 1,
            _ => {}
        }
        if e.anomaly {
            stats.anomalies += 1;
        }
        if let Some(rt) = e.response_time {
            rts.push(rt);
        }
    }

    if !rts.is_empty() {
        let sum: f64 = rts.iter().sum();
        stats.avg_response_time = Some(sum / rts.len() as f64);
        rts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = ((rts.len() as f64) * 0.95).ceil() as usize;
        stats.p95_response_time = rts.get(p95_idx.saturating_sub(1)).copied();
    }

    stats
}

// ─── Time-window filter ───────────────────────────────────────────────────────

/// Return indices of entries whose timestamp falls within `since..now`.
/// Entries without a timestamp are excluded when `since` is Some.
#[cfg(feature = "logs")]
pub fn time_window_indices(
    entries: &[LogEntry],
    since: Option<chrono::DateTime<chrono::Utc>>,
) -> Vec<usize> {
    match since {
        None => (0..entries.len()).collect(),
        Some(since) => entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.timestamp.map(|ts| ts >= since).unwrap_or(false))
            .map(|(i, _)| i)
            .collect(),
    }
}

#[cfg(not(feature = "logs"))]
pub fn time_window_indices(entries: &[LogEntry], _since: Option<()>) -> Vec<usize> {
    (0..entries.len()).collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser::parse_line;

    fn sample() -> Vec<LogEntry> {
        vec![
            parse_line(r#"{"level":"ERROR","service":"api","message":"fail"}"#, 1),
            parse_line(r#"{"level":"ERROR","service":"db","message":"fail"}"#, 2),
            parse_line(r#"{"level":"WARN","service":"api","message":"slow"}"#, 3),
            parse_line(r#"{"level":"INFO","service":"cache","message":"ok"}"#, 4),
        ]
    }

    #[test]
    fn group_by_service_counts() {
        let entries = sample();
        let map = group_by(&GroupField::Service, entries.iter());
        assert_eq!(map.get("api"), Some(&2));
        assert_eq!(map.get("db"),  Some(&1));
        assert_eq!(map.get("cache"), Some(&1));
    }

    #[test]
    fn group_by_level_counts() {
        let entries = sample();
        let map = group_by(&GroupField::Level, entries.iter());
        assert_eq!(map.get("ERROR"), Some(&2));
        assert_eq!(map.get("WARN"),  Some(&1));
        assert_eq!(map.get("INFO"),  Some(&1));
    }

    #[test]
    fn stats_error_count() {
        let entries = sample();
        let s = compute_stats(&entries);
        assert_eq!(s.errors, 2);
        assert_eq!(s.warns, 1);
        assert_eq!(s.total, 4);
    }

    #[test]
    fn stats_response_time() {
        let entries = vec![
            parse_line(r#"{"level":"INFO","message":"done","response_time":100.0}"#, 1),
            parse_line(r#"{"level":"INFO","message":"done","response_time":200.0}"#, 2),
            parse_line(r#"{"level":"INFO","message":"done","response_time":300.0}"#, 3),
        ];
        let s = compute_stats(&entries);
        let avg = s.avg_response_time.unwrap();
        assert!((avg - 200.0).abs() < 0.01);
    }
}
