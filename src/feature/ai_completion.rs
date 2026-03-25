//! AI-powered completion via external API (Cursor-style ghost text).
//!
//! Sends code context to the API and receives a completion suggestion.
//! Runs in a background thread; result is sent back via channel.

use serde::Serialize;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const API_URL: &str = "https://auto-comment.gokulakrishnanr812-492.workers.dev/";
const REQUEST_TIMEOUT_SECS: u64 = 8;
const DEFAULT_MODEL: &str = "@cf/meta/llama-3.1-8b-instruct-fast";

/// Context snapshot passed to the AI thread (no refs to main thread).
#[derive(Clone, Debug)]
pub struct AiContext {
    pub line_prefix: String,
    pub context_before: String,
    pub language: String,
    pub path: Option<String>,
    /// Override model (e.g. faster/smaller); None = DEFAULT_MODEL.
    pub model: Option<String>,
}

/// Request body for the completion API (camelCase for API).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiRequest {
    conversation_id: String,
    messages: Vec<Message>,
    model: String,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
    skip_cache: bool,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

/// Extract completion text from various API response shapes.
fn extract_completion(value: &serde_json::Value) -> Option<String> {
    let s = value
        .get("result").and_then(|r| r.get("response")).and_then(|r| r.as_str())
        .or_else(|| value.get("result").and_then(|r| r.as_str()))
        .or_else(|| value.get("response").and_then(|r| r.as_str()))
        .or_else(|| value.get("message").and_then(|m| m.as_str()))
        .or_else(|| value.get("choices").and_then(|c| c.get(0)).and_then(|m| m.get("message")).and_then(|m| m.get("content")).and_then(|c| c.as_str()))
        .or_else(|| value.get("output").and_then(|o| o.as_str()))
        .or_else(|| value.get("completion").and_then(|c| c.as_str()))
        .or_else(|| value.get("text").and_then(|t| t.as_str()))?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip markdown code block if present
    let out = if trimmed.starts_with("```") {
        let rest = trimmed.trim_start_matches("```").trim_start();
        let end = rest.find("```").unwrap_or(rest.len());
        rest[..end].trim()
    } else {
        trimmed
    };
    if out.is_empty() {
        None
    } else {
        Some(out.to_string())
    }
}

/// Strip any leading part of `suggestion` that duplicates the end of `line_prefix`
/// so we never show "INSERT INSERT INTO" when the user already typed "INSERT ".
fn strip_overlap(line_prefix: &str, suggestion: &str) -> String {
    let prefix: Vec<char> = line_prefix.chars().collect();
    let sugg: Vec<char> = suggestion.chars().collect();
    let max_overlap = prefix.len().min(sugg.len());
    for overlap in (1..=max_overlap).rev() {
        if prefix[prefix.len() - overlap..] == sugg[..overlap] {
            return sugg[overlap..].iter().collect();
        }
    }
    suggestion.to_string()
}

/// Call the API with a shared client (blocking). Returns the completion suffix or None on error.
fn fetch_with_client(client: &reqwest::blocking::Client, context: AiContext) -> Option<String> {
    let lang_note = if context.language.is_empty() || context.language == "text" {
        String::new()
    } else {
        format!(" (language: {})", context.language)
    };
    let file_note = context.path.as_deref().unwrap_or("");
    let system_prompt = format!(
        "You are a code completion assistant.{} \
        RULES: (1) The user's cursor is at the END of the text they have typed. \
        (2) Return ONLY the continuation — the exact characters that should appear NEXT. \
        (3) Do NOT repeat or include anything the user has already typed. \
        (4) Use the full file context below (tables, columns, existing statements) to suggest \
        consistent names and style. (5) No explanation, no markdown, no code blocks. \
        (6) One line or a short block; concise.",
        lang_note
    );

    let user_content = if file_note.is_empty() {
        if context.context_before.is_empty() {
            format!("Current line (cursor at end):\n{}", context.line_prefix)
        } else {
            format!(
                "Full file context above cursor:\n{}\n\nCurrent line (cursor at end):\n{}",
                context.context_before,
                context.line_prefix
            )
        }
    } else {
        if context.context_before.is_empty() {
            format!("File: {}\n\nCurrent line (cursor at end):\n{}", file_note, context.line_prefix)
        } else {
            format!(
                "File: {}\n\nContext above cursor:\n{}\n\nCurrent line (cursor at end):\n{}",
                file_note,
                context.context_before,
                context.line_prefix
            )
        }
    };

    let model = context
        .model
        .as_deref()
        .unwrap_or(DEFAULT_MODEL)
        .to_string();
    let body = ApiRequest {
        conversation_id: format!("conv_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()),
        messages: vec![
            Message { role: "system".into(), content: system_prompt.into() },
            Message { role: "user".into(), content: user_content },
        ],
        model,
        temperature: 0.3,
        max_tokens: 128,
        stream: false,
        skip_cache: false,
    };

    let resp = client
        .post(API_URL)
        .json(&body)
        .send()
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let value: serde_json::Value = resp.json().ok()?;
    let raw = extract_completion(&value)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Never duplicate what the user already typed (e.g. "INSERT " -> don't suggest "INSERT INTO...", suggest "INTO...")
    let out = strip_overlap(&context.line_prefix, trimmed);
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Spawn the single long-lived AI worker: one shared HTTP client, debounce inside the worker.
/// Receives (generation, context) on request_rx; after debounce_ms since last request, fetches and sends (generation, result) on result_tx.
pub fn spawn_ai_worker(
    request_rx: mpsc::Receiver<(u64, AiContext)>,
    result_tx: mpsc::Sender<(u64, Option<String>)>,
    debounce_ms: u64,
) {
    thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        let debounce = Duration::from_millis(debounce_ms);
        let recv_timeout = Duration::from_millis(50);
        let mut pending: Option<(u64, AiContext)> = None;
        let mut last_received = Instant::now();

        loop {
            match request_rx.recv_timeout(recv_timeout) {
                Ok((gen, ctx)) => {
                    pending = Some((gen, ctx));
                    last_received = Instant::now();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if let Some((gen, ctx)) = pending.take() {
                if last_received.elapsed() >= debounce {
                    let result = fetch_with_client(&client, ctx);
                    let _ = result_tx.send((gen, result));
                } else {
                    pending = Some((gen, ctx));
                }
            }
        }
    });
}
