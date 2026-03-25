//! Query engine — parses slash commands and applies filters to log entries.
//!
//! Supported syntax:
//!   /error [last N min|h]
//!   /warn  [last N min|h]
//!   /info  [last N min|h]
//!   /status <code>           – match HTTP-like status anywhere in message
//!   /slow > <N>ms|s          – filter by response_time
//!   /group by <field>        – aggregate view (level | service | status)
//!   /search <term>           – regex search across full raw line
//!   > <text>                 – natural-language AI query (handled in ui.rs)

use regex::Regex;
use std::sync::OnceLock;

use super::parser::{LogEntry, LogLevel};

// ─── TimeWindow ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TimeWindow {
    LastN { value: u64, unit: TimeUnit },
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeUnit { Seconds, Minutes, Hours }

impl TimeWindow {
    #[cfg(feature = "logs")]
    pub fn start_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::{Duration, Utc};
        match self {
            TimeWindow::All => None,
            TimeWindow::LastN { value, unit } => {
                let d = match unit {
                    TimeUnit::Seconds => Duration::seconds(*value as i64),
                    TimeUnit::Minutes => Duration::minutes(*value as i64),
                    TimeUnit::Hours   => Duration::hours(*value as i64),
                };
                Some(Utc::now() - d)
            }
        }
    }
    #[cfg(not(feature = "logs"))]
    pub fn start_time(&self) -> Option<()> { None }
}

// ─── GroupField ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum GroupField { Level, Service, Status }

// ─── Query ────────────────────────────────────────────────────────────────────

/// A compiled, reusable query.
#[derive(Debug, Clone)]
pub enum Query {
    /// Filter by log level, optional time window.
    Level {
        level: LogLevel,
        window: TimeWindow,
    },
    /// Filter by HTTP status code appearing in the message/raw.
    Status(u16),
    /// Filter entries where response_time >= threshold_ms.
    Slow { threshold_ms: f64 },
    /// Group-by summary mode (no filter — changes display).
    GroupBy(GroupField),
    /// Arbitrary regex against the raw line.
    Search(Regex),
    /// Natural-language AI query — passed straight through to the AI layer.
    Ai(String),
    /// No active query (show all entries).
    None,
}

impl Query {
    /// Parse a query from the interactive input bar.
    /// Returns `None` if the string is empty / whitespace.
    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        if s.is_empty() {
            return Some(Query::None);
        }

        // AI / NL query
        if let Some(rest) = s.strip_prefix('>') {
            return Some(Query::Ai(rest.trim().to_string()));
        }

        if let Some(rest) = s.strip_prefix('/') {
            return parse_slash(rest.trim());
        }

        // bare text → implicit search
        parse_search(s)
    }
}

fn parse_slash(s: &str) -> Option<Query> {
    let lower = s.to_lowercase();
    let parts: Vec<&str> = lower.splitn(2, char::is_whitespace).collect();
    let cmd = parts[0];
    let args = parts.get(1).copied().unwrap_or("").trim();

    // Level filters
    let level = match cmd {
        "error" | "err"   => Some(LogLevel::Error),
        "warn" | "warning" => Some(LogLevel::Warn),
        "info"             => Some(LogLevel::Info),
        "debug"            => Some(LogLevel::Debug),
        "trace"            => Some(LogLevel::Trace),
        _ => None,
    };
    if let Some(level) = level {
        let window = parse_window(args).unwrap_or(TimeWindow::All);
        return Some(Query::Level { level, window });
    }

    // status <code>
    if cmd == "status" {
        if let Ok(code) = args.parse::<u16>() {
            return Some(Query::Status(code));
        }
    }

    // slow > <N>ms|s
    if cmd == "slow" {
        if let Some(ms) = parse_slow(args) {
            return Some(Query::Slow { threshold_ms: ms });
        }
    }

    // group by <field>
    if cmd == "group" {
        let rest = args.strip_prefix("by").map(str::trim).unwrap_or(args);
        let field = match rest {
            "service" | "svc" | "app" => Some(GroupField::Service),
            "level" | "lvl"           => Some(GroupField::Level),
            "status"                  => Some(GroupField::Status),
            _ => None,
        };
        if let Some(f) = field {
            return Some(Query::GroupBy(f));
        }
    }

    // search <term> or /term
    if cmd == "search" || cmd == "grep" {
        return parse_search(args);
    }

    // Treat the whole slash expression as a regex search
    parse_search(s)
}

fn parse_window(args: &str) -> Option<TimeWindow> {
    // "last 10 min", "last 1h", "last 2 hours"
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)last\s+(\d+)\s*(s(?:ec(?:onds?)?)?|m(?:in(?:utes?)?)?|h(?:ours?)?)").unwrap()
    });
    let caps = re.captures(args)?;
    let val: u64 = caps[1].parse().ok()?;
    let unit = match caps[2].chars().next()? {
        's' | 'S' => TimeUnit::Seconds,
        'm' | 'M' => TimeUnit::Minutes,
        _ => TimeUnit::Hours,
    };
    Some(TimeWindow::LastN { value: val, unit })
}

fn parse_slow(args: &str) -> Option<f64> {
    // "> 200ms", "> 1.5s", ">500ms"
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)>?\s*(\d+(?:\.\d+)?)\s*(ms|s)\b").unwrap()
    });
    let caps = re.captures(args)?;
    let val: f64 = caps[1].parse().ok()?;
    let unit = &caps[2];
    if unit.eq_ignore_ascii_case("s") {
        Some(val * 1000.0)
    } else {
        Some(val)
    }
}

fn parse_search(s: &str) -> Option<Query> {
    if s.is_empty() {
        return Some(Query::None);
    }
    // Try as regex; fall back to literal
    let re = Regex::new(s)
        .or_else(|_| Regex::new(&regex::escape(s)))
        .ok()?;
    Some(Query::Search(re))
}

// ─── Filter ───────────────────────────────────────────────────────────────────

/// Apply a query filter to a slice of entries, returning matching indices.
pub fn filter_indices(entries: &[LogEntry], query: &Query) -> Vec<usize> {
    match query {
        Query::None | Query::GroupBy(_) | Query::Ai(_) => {
            (0..entries.len()).collect()
        }

        Query::Level { level, window } => {
            entries.iter().enumerate()
                .filter(|(_, e)| {
                    let level_match = &e.level <= level; // Error <= Error, Warn etc.
                    // For level queries we match the exact level and above
                    let exact = &e.level == level;
                    let _ = level_match;
                    if !exact {
                        return false;
                    }
                    #[cfg(feature = "logs")]
                    if let Some(since) = window.start_time() {
                        if let Some(ts) = e.timestamp {
                            return ts >= since;
                        }
                        return false; // no timestamp → exclude from time-windowed query
                    }
                    true
                })
                .map(|(i, _)| i)
                .collect()
        }

        Query::Status(code) => {
            let pattern = code.to_string();
            entries.iter().enumerate()
                .filter(|(_, e)| e.raw.contains(&pattern))
                .map(|(i, _)| i)
                .collect()
        }

        Query::Slow { threshold_ms } => {
            entries.iter().enumerate()
                .filter(|(_, e)| e.response_time.map(|rt| rt >= *threshold_ms).unwrap_or(false))
                .map(|(i, _)| i)
                .collect()
        }

        Query::Search(re) => {
            entries.iter().enumerate()
                .filter(|(_, e)| re.is_match(&e.raw))
                .map(|(i, _)| i)
                .collect()
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::parser::parse_line;

    fn make_entries() -> Vec<LogEntry> {
        vec![
            parse_line(r#"{"level":"ERROR","message":"db timeout","response_time":3000.0}"#, 1),
            parse_line(r#"{"level":"WARN","message":"high cpu"}"#, 2),
            parse_line(r#"{"level":"INFO","message":"server start"}"#, 3),
            parse_line("request handled in 123ms", 4),
            parse_line("GET /path 500 internal error", 5),
        ]
    }

    #[test]
    fn filter_error() {
        let entries = make_entries();
        let q = Query::parse("/error").unwrap();
        let idx = filter_indices(&entries, &q);
        assert_eq!(idx, vec![0]);
    }

    #[test]
    fn filter_warn() {
        let entries = make_entries();
        let q = Query::parse("/warn").unwrap();
        let idx = filter_indices(&entries, &q);
        assert_eq!(idx, vec![1]);
    }

    #[test]
    fn filter_slow() {
        let entries = make_entries();
        let q = Query::parse("/slow > 200ms").unwrap();
        let idx = filter_indices(&entries, &q);
        assert_eq!(idx, vec![0]); // 3000ms
    }

    #[test]
    fn filter_status() {
        let entries = make_entries();
        let q = Query::parse("/status 500").unwrap();
        let idx = filter_indices(&entries, &q);
        assert!(idx.contains(&4));
    }

    #[test]
    fn filter_search_regex() {
        let entries = make_entries();
        let q = Query::parse("/search timeout").unwrap();
        let idx = filter_indices(&entries, &q);
        assert!(idx.contains(&0));
    }

    #[test]
    fn ai_query_passthrough() {
        let q = Query::parse("> summarize errors").unwrap();
        assert!(matches!(q, Query::Ai(_)));
    }

    #[test]
    fn empty_is_none() {
        let q = Query::parse("   ").unwrap();
        assert!(matches!(q, Query::None));
    }

    #[test]
    fn slow_threshold_seconds() {
        let q = Query::parse("/slow > 2s").unwrap();
        assert!(matches!(q, Query::Slow { threshold_ms } if (threshold_ms - 2000.0).abs() < 0.01));
    }

    #[test]
    fn window_parse_minutes() {
        let w = parse_window("last 10 min").unwrap();
        assert_eq!(w, TimeWindow::LastN { value: 10, unit: TimeUnit::Minutes });
    }

    #[test]
    fn group_by_service() {
        let q = Query::parse("/group by service").unwrap();
        assert!(matches!(q, Query::GroupBy(GroupField::Service)));
    }
}
