//! AI integration for the Smart Log Explorer.
//!
//! Sends a log snippet + natural-language prompt to Gemini and returns an
//! `AiReport` with summary, root-cause, and suggested actions.
//!
//! This module re-uses the same blocking `ureq` stack as the main editor's
//! `gemini_chat` module, so it doesn't require a separate async runtime.

#[cfg(feature = "ai")]
use ureq;
use std::time::Duration;

use crate::feature::gemini_chat::{extract_response_text, GeminiError};

// ─── AiReport ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AiReport {
    pub summary:     String,
    pub root_cause:  String,
    pub suggestions: Vec<String>,
    pub raw:         String,
}

impl AiReport {
    /// Render as a human-readable string for display in the TUI panel.
    pub fn to_display(&self) -> String {
        let mut out = String::new();

        out.push_str("📋 Summary\n");
        out.push_str(&self.summary);
        out.push_str("\n\n");

        if !self.root_cause.is_empty() {
            out.push_str("🔍 Root Cause\n");
            out.push_str(&self.root_cause);
            out.push_str("\n\n");
        }

        if !self.suggestions.is_empty() {
            out.push_str("💡 Suggestions\n");
            for (i, s) in self.suggestions.iter().enumerate() {
                out.push_str(&format!("  {}. {}\n", i + 1, s));
            }
        }

        out
    }
}

// ─── Prompt building ──────────────────────────────────────────────────────────

const MAX_LOG_BYTES: usize = 12_000;
const TIMEOUT_SECS: u64 = 90;

fn build_prompt(nl_query: &str, log_snippet: &str) -> String {
    let trimmed_log = if log_snippet.len() > MAX_LOG_BYTES {
        &log_snippet[log_snippet.len() - MAX_LOG_BYTES..]
    } else {
        log_snippet
    };

    format!(
        "You are a senior DevOps engineer analyzing application or system logs.\n\
         The user asked: \"{}\"\n\n\
         Here is the relevant log data:\n\
         ```\n{}\n```\n\n\
         Please respond with:\n\
         1. A concise **Summary** (2-3 sentences)\n\
         2. The likely **Root Cause** (1-2 sentences)\n\
         3. **Suggestions** (numbered list, 2-4 actionable items)\n\n\
         Use plain text without markdown headers. Separate each section with a blank line.",
        nl_query, trimmed_log
    )
}

// ─── Parse model response ─────────────────────────────────────────────────────

fn parse_report(raw: &str) -> AiReport {
    // Try to split on numbered list or obvious section breaks.
    let parts: Vec<&str> = raw.splitn(3, "\n\n").collect();

    let summary = parts.first().copied().unwrap_or("").trim().to_string();

    let root_cause = parts
        .get(1)
        .copied()
        .unwrap_or("")
        .trim()
        .trim_start_matches("Root Cause:")
        .trim()
        .to_string();

    let suggestions_raw = parts.get(2).copied().unwrap_or("").trim();
    let suggestions: Vec<String> = suggestions_raw
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            // Strip leading "1. ", "- ", "• " etc.
            let l = l.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '•' || c == '*')
                     .trim();
            if l.is_empty() { None } else { Some(l.to_string()) }
        })
        .collect();

    AiReport {
        summary,
        root_cause,
        suggestions,
        raw: raw.to_string(),
    }
}

// ─── HTTP call ────────────────────────────────────────────────────────────────

#[cfg(feature = "ai")]
pub fn query(
    nl_query:    &str,
    log_snippet: &str,
    api_key:     &str,
    model_id:    &str,
) -> Result<AiReport, GeminiError> {
    use serde_json::json;

    let prompt = build_prompt(nl_query, log_snippet);

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model_id, api_key
    );

    let body = json!({
        "contents": [{"role": "user", "parts": [{"text": prompt}]}]
    });

    let agent = ureq::Agent::new();
    let resp = agent
        .post(&url)
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .send_json(body)
        .map_err(|e| GeminiError::Network(e.to_string()))?;

    let status = resp.status();
    let text = resp.into_string().map_err(|e| GeminiError::Network(e.to_string()))?;

    if !(200..300).contains(&(status as i32)) {
        return Err(GeminiError::Http(status, text.chars().take(256).collect()));
    }

    let model_text = extract_response_text(&text)?;
    Ok(parse_report(&model_text))
}

#[cfg(not(feature = "ai"))]
pub fn query(
    _nl_query:    &str,
    _log_snippet: &str,
    _api_key:     &str,
    _model_id:    &str,
) -> Result<AiReport, GeminiError> {
    Err(GeminiError::Api(
        "AI features require building with --features ai".to_string()
    ))
}

// ─── Async wrapper ────────────────────────────────────────────────────────────

/// Run the blocking Gemini call on a tokio blocking thread, returning a future.
#[cfg(feature = "logs")]
pub async fn query_async(
    nl_query:    String,
    log_snippet: String,
    api_key:     String,
    model_id:    String,
) -> Result<AiReport, GeminiError> {
    tokio::task::spawn_blocking(move || query(&nl_query, &log_snippet, &api_key, &model_id))
        .await
        .map_err(|e| GeminiError::Network(e.to_string()))?
}
