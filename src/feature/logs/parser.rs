//! Log line parser — supports plain-text (syslog/nginx/generic) and JSON formats.
//!
//! Each line is attempted as JSON first; on failure it falls through to a cascade of
//! plain-text regexes.  All fields are optional except `raw` and `line_no`.

use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

// ─── Level ───────────────────────────────────────────────────────────────────

/// Severity level, ordered from most severe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Unknown,
}

impl LogLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "ERROR" | "ERR" | "CRIT" | "CRITICAL" | "FATAL" | "EMERG" | "ALERT" => {
                LogLevel::Error
            }
            "WARN" | "WARNING" => LogLevel::Warn,
            "INFO" | "NOTICE" | "INFORMATION" => LogLevel::Info,
            "DEBUG" | "DBG" => LogLevel::Debug,
            "TRACE" => LogLevel::Trace,
            _ => LogLevel::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn  => "WARN ",
            LogLevel::Info  => "INFO ",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
            LogLevel::Unknown => "?    ",
        }
    }
}

// ─── LogEntry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Parsed timestamp (UTC).  `None` when unparseable.
    #[cfg(feature = "logs")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub level: LogLevel,
    pub service: Option<String>,
    pub message: String,
    /// Response / duration in **milliseconds**.
    pub response_time: Option<f64>,
    pub raw: String,
    pub line_no: u64,
    /// Set by the anomaly detector.
    pub anomaly: bool,
}

impl LogEntry {
    fn unknown(raw: String, line_no: u64) -> Self {
        LogEntry {
            #[cfg(feature = "logs")]
            timestamp: None,
            level: LogLevel::Unknown,
            service: None,
            message: raw.clone(),
            response_time: None,
            raw,
            line_no,
            anomaly: false,
        }
    }
}

// ─── Regex cache ─────────────────────────────────────────────────────────────

fn syslog_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // e.g. "Nov  1 12:00:01 hostname servicename[123]: message"
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            ^(?P<month>[A-Za-z]{3})\s+(?P<day>\d{1,2})\s+(?P<hms>\d{2}:\d{2}:\d{2})\s+
            (?P<host>\S+)\s+
            (?P<svc>[^:\[]+?)(?:\[\d+\])?
            :\s+(?P<msg>.*)$",
        )
        .unwrap()
    })
}

fn iso_generic_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // e.g. "2024-01-15T12:00:00Z [ERROR] service: message"
    // or   "2024-01-15 12:00:00 ERROR some message"
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            ^(?P<ts>\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\s*
            [\[\s]*(?P<lvl>ERROR|WARN(?:ING)?|INFO|DEBUG|TRACE|NOTICE|CRITICAL|FATAL|ERR)?[\]\s]*
            (?P<rest>.*)$",
        )
        .unwrap()
    })
}

fn bracket_level_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // e.g.  "[INFO] doing something"  or  "INFO: doing something"
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)[\[\s](?P<lvl>error|warn(?:ing)?|info|debug|trace|notice|critical|fatal|err)[\]\s:]",
        )
        .unwrap()
    })
}

fn nginx_access_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // nginx: 127.0.0.1 - - [15/Jan/2024:12:00:00 +0000] "GET /path HTTP/1.1" 200 1234 "-" "-" 0.123
    RE.get_or_init(|| {
        Regex::new(
            r#"(?x)
            ^\S+\s+-\s+-\s+\[(?P<ts>[^\]]+)\]\s+
            "(?P<method>[A-Z]+)\s+(?P<path>\S+)[^"]*"\s+
            (?P<status>\d{3})\s+\d+
            (?:\s+"[^"]*"\s+"[^"]*"\s+(?P<rt>\d+\.\d+))?"#,
        )
        .unwrap()
    })
}

fn duration_in_message_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // looks for "123ms", "1.5s", "200 ms", etc. anywhere in the message
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?P<val>\d+(?:\.\d+)?)\s*(?P<unit>ms|s)\b").unwrap()
    })
}

// ─── Timestamp helpers ───────────────────────────────────────────────────────

#[cfg(feature = "logs")]
fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, NaiveDateTime, Utc};
    // RFC3339 / ISO8601
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // naive datetime
    for fmt in &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(ndt.and_utc());
        }
    }
    None
}

#[cfg(not(feature = "logs"))]
fn parse_ts(_s: &str) -> Option<()> { None }

// ─── Response-time extraction ─────────────────────────────────────────────────

/// Extract response time in milliseconds from a string (e.g. "123ms", "1.5 s").
fn extract_response_time(s: &str) -> Option<f64> {
    let caps = duration_in_message_re().captures(s)?;
    let val: f64 = caps["val"].parse().ok()?;
    let unit = &caps["unit"];
    if unit.eq_ignore_ascii_case("s") {
        Some(val * 1000.0)
    } else {
        Some(val)
    }
}

// ─── Level extraction from strings ───────────────────────────────────────────

fn level_from_status(status: u16) -> LogLevel {
    match status {
        200..=399 => LogLevel::Info,
        400..=499 => LogLevel::Warn,
        _ => LogLevel::Error,
    }
}

// ─── JSON parser ─────────────────────────────────────────────────────────────

fn field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(s) = v.get(k).and_then(|v| v.as_str()) {
            return Some(s);
        }
    }
    None
}

fn field_f64(v: &Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(n) = v.get(k).and_then(|v| v.as_f64()) {
            return Some(n);
        }
        // also try string that parses
        if let Some(s) = v.get(k).and_then(|v| v.as_str()) {
            if let Ok(n) = s.parse::<f64>() {
                return Some(n);
            }
        }
    }
    None
}

fn parse_json_line(raw: &str, line_no: u64) -> Option<LogEntry> {
    let v: Value = serde_json::from_str(raw.trim()).ok()?;
    if !v.is_object() {
        return None;
    }

    let ts_str = field(&v, &["timestamp", "time", "@timestamp", "ts", "date"]);
    #[cfg(feature = "logs")]
    let timestamp = ts_str.and_then(parse_ts);

    let level_str = field(&v, &["level", "severity", "lvl", "log_level", "loglevel"]);
    let level = level_str
        .map(LogLevel::from_str)
        .unwrap_or(LogLevel::Unknown);

    // HTTP status may imply level
    let level = if level == LogLevel::Unknown {
        v.get("status")
            .and_then(|s| s.as_u64())
            .map(|s| level_from_status(s as u16))
            .unwrap_or(LogLevel::Unknown)
    } else {
        level
    };

    let service = field(&v, &["service", "app", "application", "logger", "name"])
        .map(|s| s.to_string());

    let message = field(&v, &["message", "msg", "text", "body", "event"])
        .unwrap_or(raw)
        .to_string();

    // response time: prefer dedicated field (ms); also try seconds field
    let response_time = field_f64(&v, &["response_time", "duration", "elapsed", "latency"])
        .or_else(|| field_f64(&v, &["duration_s", "elapsed_s"]).map(|s| s * 1000.0))
        .or_else(|| extract_response_time(&message));

    Some(LogEntry {
        #[cfg(feature = "logs")]
        timestamp,
        level,
        service,
        message,
        response_time,
        raw: raw.to_string(),
        line_no,
        anomaly: false,
    })
}

// ─── Plain-text parser ────────────────────────────────────────────────────────

fn parse_plain_line(raw: &str, line_no: u64) -> LogEntry {
    // 1. nginx access log
    if let Some(caps) = nginx_access_re().captures(raw) {
        let status: u16 = caps.name("status")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let response_time = caps.name("rt")
            .and_then(|m| m.as_str().parse::<f64>().ok())
            .map(|s| s * 1000.0); // seconds → ms

        let ts_str = caps.name("ts").map(|m| m.as_str()).unwrap_or("");
        #[cfg(feature = "logs")]
        let timestamp = parse_ts(ts_str);

        let path = caps.name("path").map(|m| m.as_str()).unwrap_or("");
        let method = caps.name("method").map(|m| m.as_str()).unwrap_or("");
        return LogEntry {
            #[cfg(feature = "logs")]
            timestamp,
            level: level_from_status(status),
            service: Some("nginx".to_string()),
            message: format!("{} {} {}", method, path, status),
            response_time,
            raw: raw.to_string(),
            line_no,
            anomaly: false,
        };
    }

    // 2. syslog
    if let Some(caps) = syslog_re().captures(raw) {
        let svc = caps.name("svc").map(|m| m.as_str().trim().to_string());
        let msg = caps.name("msg").map(|m| m.as_str()).unwrap_or(raw);
        let level = bracket_level_re()
            .captures(msg)
            .and_then(|c| c.name("lvl"))
            .map(|m| LogLevel::from_str(m.as_str()))
            .unwrap_or(LogLevel::Unknown);

        let hms = caps.name("hms").map(|m| m.as_str()).unwrap_or("");
        let month = caps.name("month").map(|m| m.as_str()).unwrap_or("Jan");
        let day = caps.name("day").map(|m| m.as_str()).unwrap_or("1");
        let ts_candidate = format!("2024-{}-{} {}", month_num(month), day, hms);
        #[cfg(feature = "logs")]
        let timestamp = parse_ts(&ts_candidate);

        return LogEntry {
            #[cfg(feature = "logs")]
            timestamp,
            level,
            service: svc,
            message: msg.to_string(),
            response_time: extract_response_time(msg),
            raw: raw.to_string(),
            line_no,
            anomaly: false,
        };
    }

    // 3. ISO timestamp + optional level
    if let Some(caps) = iso_generic_re().captures(raw) {
        let ts_str = caps.name("ts").map(|m| m.as_str()).unwrap_or("");
        #[cfg(feature = "logs")]
        let timestamp = parse_ts(ts_str);

        let level = caps
            .name("lvl")
            .map(|m| LogLevel::from_str(m.as_str()))
            .unwrap_or_else(|| {
                bracket_level_re()
                    .captures(raw)
                    .and_then(|c| c.name("lvl"))
                    .map(|m| LogLevel::from_str(m.as_str()))
                    .unwrap_or(LogLevel::Unknown)
            });
        let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or(raw);
        return LogEntry {
            #[cfg(feature = "logs")]
            timestamp,
            level,
            service: None,
            message: rest.to_string(),
            response_time: extract_response_time(rest),
            raw: raw.to_string(),
            line_no,
            anomaly: false,
        };
    }

    // 4. bracket level anywhere
    if let Some(caps) = bracket_level_re().captures(raw) {
        let level = caps.name("lvl")
            .map(|m| LogLevel::from_str(m.as_str()))
            .unwrap_or(LogLevel::Unknown);
        return LogEntry {
            #[cfg(feature = "logs")]
            timestamp: None,
            level,
            service: None,
            message: raw.to_string(),
            response_time: extract_response_time(raw),
            raw: raw.to_string(),
            line_no,
            anomaly: false,
        };
    }

    // 5. fallback
    LogEntry::unknown(raw.to_string(), line_no)
}

fn month_num(m: &str) -> &'static str {
    match m {
        "Jan" => "01", "Feb" => "02", "Mar" => "03", "Apr" => "04",
        "May" => "05", "Jun" => "06", "Jul" => "07", "Aug" => "08",
        "Sep" => "09", "Oct" => "10", "Nov" => "11", _   => "12",
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Parse a single raw log line.
pub fn parse_line(raw: &str, line_no: u64) -> LogEntry {
    if raw.trim_start().starts_with('{') {
        if let Some(e) = parse_json_line(raw, line_no) {
            return e;
        }
    }
    parse_plain_line(raw, line_no)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_error_line() {
        let raw = r#"{"timestamp":"2024-01-15T12:00:00Z","level":"ERROR","service":"api","message":"db timeout","response_time":2500.0}"#;
        let e = parse_line(raw, 1);
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.service.as_deref(), Some("api"));
        assert!((e.response_time.unwrap() - 2500.0).abs() < 0.01);
    }

    #[test]
    fn test_json_warn_line() {
        let raw = r#"{"time":"2024-03-01T09:00:00Z","severity":"WARN","msg":"high memory"}"#;
        let e = parse_line(raw, 2);
        assert_eq!(e.level, LogLevel::Warn);
    }

    #[test]
    fn test_json_fallback_to_plain() {
        let raw = "not json at all [ERROR] something bad";
        let e = parse_line(raw, 3);
        assert_eq!(e.level, LogLevel::Error);
    }

    #[test]
    fn test_iso_timestamp_info() {
        let raw = "2024-01-15 12:00:00 INFO server started";
        let e = parse_line(raw, 4);
        assert_eq!(e.level, LogLevel::Info);
    }

    #[test]
    fn test_response_time_extraction_ms() {
        let raw = r#"{"level":"INFO","message":"request done in 123ms"}"#;
        let e = parse_line(raw, 5);
        assert!((e.response_time.unwrap() - 123.0).abs() < 0.01);
    }

    #[test]
    fn test_response_time_extraction_s() {
        let raw = "2024-01-15 12:00:00 INFO request completed in 1.5s";
        let e = parse_line(raw, 6);
        assert!((e.response_time.unwrap() - 1500.0).abs() < 0.01);
    }

    #[test]
    fn test_level_from_str_variants() {
        assert_eq!(LogLevel::from_str("CRITICAL"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("WARNING"), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("notice"), LogLevel::Info);
        assert_eq!(LogLevel::from_str("gibberish"), LogLevel::Unknown);
    }
}
