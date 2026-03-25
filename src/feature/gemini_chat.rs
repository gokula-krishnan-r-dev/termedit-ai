//! Gemini REST `generateContent` for the AI assistant panel (blocking worker thread).

use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::thread;
#[cfg(feature = "ai")]
use std::time::Duration;

/// Models offered in the UI and via `--list-gemini-models` / `--ai-chat-model`.
/// Any valid Google `models/{id}` id works if passed through config or CLI (even if not listed here).
pub const GEMINI_CHAT_MODELS: &[&str] = &[
    "gemini-2.5-flash",
    "gemini-2.5-pro",
    "gemini-2.0-flash",
    "gemini-2.5-flash-preview-05-20",
    "gemini-1.5-flash",
    "gemini-1.5-pro",
    "gemini-2.5-flash-lite",
];

/// Prefilled prompt for **AI: Brainstorm ideas** (command palette / Ctrl/Cmd+Shift+U).
pub fn brainstorm_user_prompt(file_display: &str, language: &str) -> String {
    format!(
        "Brainstorm 5 concise, numbered ideas (1–5) for improving or extending my work. \
Each idea one short sentence. Current file: `{}`. Language: {}.\n\
(Optional: replace this line with a focus area — e.g. \"performance\", \"UX\", \"new language support\")",
        file_display,
        language
    )
}

/// Default chat model id when config/CLI does not set one.
pub fn default_chat_model_id() -> &'static str {
    GEMINI_CHAT_MODELS[0]
}

/// Resolve model id from config.toml or CLI (`--ai-chat-model`): trim; empty → first built-in.
pub fn resolve_chat_model_id(config_or_cli: Option<&str>) -> String {
    match config_or_cli.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => default_chat_model_id().to_string(),
    }
}

/// Human-readable list for `termedit --list-gemini-models`.
pub fn models_list_text() -> String {
    let mut out = String::from("Gemini model ids for the AI assistant (Tab cycles these in the panel):\n\n");
    for m in GEMINI_CHAT_MODELS {
        out.push_str(m);
        out.push('\n');
    }
    out.push_str("\nYou can set any valid API model id via --ai-chat-model or ai_chat_model in config.toml.\n");
    out.push_str("API key: environment variable GEMINI_API_KEY (recommended) or gemini_api_key in config.\n");
    out
}

/// Index in [`GEMINI_CHAT_MODELS`] for Tab cycling; `None` if custom id.
pub fn preset_model_index(name: &str) -> Option<usize> {
    GEMINI_CHAT_MODELS.iter().position(|&m| m == name)
}

/// Max UTF-8 bytes of conversation (user+model turns) sent to the API (after truncation).
pub const CHAT_HISTORY_MAX_BYTES: usize = 48_000;

const API_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub enum ChatRole {
    User,
    Model,
}

#[derive(Debug, Clone)]
pub struct GeminiTurn {
    pub role: ChatRole,
    pub text: String,
}

impl GeminiTurn {
    fn api_role(&self) -> &'static str {
        match self.role {
            ChatRole::User => "user",
            ChatRole::Model => "model",
        }
    }
}

#[derive(Debug)]
pub enum GeminiError {
    Http(u16, String),
    Api(String),
    Network(String),
    Parse(String),
    NoContent,
}

impl std::fmt::Display for GeminiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeminiError::Http(code, msg) => write!(f, "HTTP {}: {}", code, msg),
            GeminiError::Api(msg) => write!(f, "{}", msg),
            GeminiError::Network(msg) => write!(f, "{}", msg),
            GeminiError::Parse(msg) => write!(f, "{}", msg),
            GeminiError::NoContent => write!(f, "Empty response from model"),
        }
    }
}

impl std::error::Error for GeminiError {}

/// Serialized request to `models/{model}:generateContent`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction<'a>>,
    contents: Vec<Content<'a>>,
}

#[derive(Serialize)]
struct SystemInstruction<'a> {
    parts: [Part<'a>; 1],
}

#[derive(Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<Part<'a>>,
}

#[derive(Serialize)]
struct Part<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
    error: Option<ApiErrorInner>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<ContentBody>,
}

#[derive(Deserialize)]
struct ContentBody {
    parts: Option<Vec<PartBody>>,
}

#[derive(Deserialize)]
struct PartBody {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiErrorInner {
    message: Option<String>,
    code: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct GeminiChatRequest {
    pub api_key: String,
    pub model_id: String,
    pub system_instruction: String,
    pub turns: Vec<GeminiTurn>,
}

/// Drop oldest turns until total UTF-8 size of `text` fields is ≤ `max_bytes`.
pub fn truncate_turns_for_budget(turns: &[GeminiTurn], max_bytes: usize) -> Vec<GeminiTurn> {
    if turns.is_empty() {
        return vec![];
    }
    let mut out: Vec<GeminiTurn> = turns.to_vec();
    loop {
        let total: usize = out.iter().map(|t| t.text.len()).sum();
        if total <= max_bytes || out.len() <= 1 {
            break;
        }
        out.remove(0);
    }
    while let Some(t) = out.first() {
        if matches!(t.role, ChatRole::Model) {
            out.remove(0);
        } else {
            break;
        }
    }
    out
}

fn build_body<'a>(
    system_instruction: &'a str,
    turns: &'a [GeminiTurn],
) -> GenerateContentBody<'a> {
    let contents: Vec<Content<'a>> = turns
        .iter()
        .map(|t| Content {
            role: t.api_role(),
            parts: vec![Part { text: t.text.as_str() }],
        })
        .collect();

    let system_instruction = if system_instruction.is_empty() {
        None
    } else {
        Some(SystemInstruction {
            parts: [Part {
                text: system_instruction,
            }],
        })
    };

    GenerateContentBody {
        system_instruction,
        contents,
    }
}

pub fn default_system_instruction(file_label: &str, language: &str) -> String {
    format!(
        "You are a concise coding assistant inside a terminal editor (TermEdit). \
The user may paste code or ask for edits. Prefer clear, actionable answers. \
Do not add a preamble like \"Certainly!\" unless the user asks for chatty tone. \
Use markdown for code fences when showing multi-line code. \
Current buffer: {}. Language: {}.",
        file_label, language
    )
}

/// Extract assistant text from successful JSON body.
pub fn extract_response_text(json: &str) -> Result<String, GeminiError> {
    let v: GenerateResponse =
        serde_json::from_str(json).map_err(|e| GeminiError::Parse(e.to_string()))?;
    if let Some(err) = v.error {
        let msg = err.message.unwrap_or_else(|| "API error".to_string());
        return Err(GeminiError::Api(msg));
    }
    let Some(cands) = v.candidates else {
        return Err(GeminiError::NoContent);
    };
    let Some(first) = cands.first() else {
        return Err(GeminiError::NoContent);
    };
    let Some(content) = &first.content else {
        return Err(GeminiError::NoContent);
    };
    let Some(parts) = &content.parts else {
        return Err(GeminiError::NoContent);
    };
    let mut out = String::new();
    for p in parts {
        if let Some(t) = &p.text {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(t);
        }
    }
    if out.trim().is_empty() {
        Err(GeminiError::NoContent)
    } else {
        Ok(out)
    }
}

#[cfg(feature = "ai")]
fn generate(
    agent: &ureq::Agent,
    api_key: &str,
    model_id: &str,
    system_instruction: &str,
    turns: &[GeminiTurn],
) -> Result<String, GeminiError> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model_id, api_key
    );
    let body = build_body(system_instruction, turns);
    let resp = agent
        .post(&url)
        .timeout(Duration::from_secs(API_TIMEOUT_SECS))
        .send_json(&body)
        .map_err(|e| GeminiError::Network(e.to_string()))?;

    let status = resp.status();
    let text = resp
        .into_string()
        .map_err(|e| GeminiError::Network(e.to_string()))?;

    if status < 200 || status >= 300 {
        if let Ok(v) = serde_json::from_str::<GenerateResponse>(&text) {
            if let Some(err) = v.error {
                let msg = err.message.unwrap_or_else(|| format!("HTTP {}", status));
                return Err(GeminiError::Http(status, msg));
            }
        }
        let msg = text.chars().take(512).collect::<String>();
        return Err(GeminiError::Http(status, msg));
    }

    extract_response_text(&text)
}

#[cfg(feature = "ai")]
pub fn spawn_gemini_worker(
    request_rx: mpsc::Receiver<(u64, GeminiChatRequest)>,
    result_tx: mpsc::Sender<(u64, Result<String, GeminiError>)>,
) {
    thread::spawn(move || {
        let agent = ureq::Agent::new();

        while let Ok((id, req)) = request_rx.recv() {
            let turns = truncate_turns_for_budget(&req.turns, CHAT_HISTORY_MAX_BYTES);
            let res = generate(
                &agent,
                &req.api_key,
                &req.model_id,
                &req.system_instruction,
                &turns,
            );
            let _ = result_tx.send((id, res));
        }
    });
}

#[cfg(not(feature = "ai"))]
pub fn spawn_gemini_worker(
    request_rx: mpsc::Receiver<(u64, GeminiChatRequest)>,
    _result_tx: mpsc::Sender<(u64, Result<String, GeminiError>)>,
) {
    thread::spawn(move || {
        while request_rx.recv().is_ok() {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_drops_from_start() {
        let turns = vec![
            GeminiTurn {
                role: ChatRole::User,
                text: "a".repeat(100),
            },
            GeminiTurn {
                role: ChatRole::Model,
                text: "b".repeat(100),
            },
            GeminiTurn {
                role: ChatRole::User,
                text: "c".repeat(50),
            },
        ];
        let cut = truncate_turns_for_budget(&turns, 160);
        assert!(cut.len() < turns.len());
        assert!(cut.last().unwrap().text.starts_with('c'));
    }

    #[test]
    fn extract_candidate_text() {
        let json = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}]}}]}"#;
        assert_eq!(extract_response_text(json).unwrap(), "Hello");
    }

    #[test]
    fn extract_api_error() {
        let json = r#"{"error":{"message":"bad request","code":400}}"#;
        assert!(matches!(
            extract_response_text(json),
            Err(GeminiError::Api(_))
        ));
    }

    #[test]
    fn preset_model_index_known_and_custom() {
        assert!(preset_model_index("gemini-1.5-flash").is_some());
        assert!(preset_model_index("custom-model-xyz").is_none());
    }

    #[test]
    fn truncate_strips_leading_model_turn() {
        let turns = vec![
            GeminiTurn {
                role: ChatRole::Model,
                text: "orphan".into(),
            },
            GeminiTurn {
                role: ChatRole::User,
                text: "hi".into(),
            },
        ];
        let cut = truncate_turns_for_budget(&turns, 10_000);
        assert!(matches!(cut.first().unwrap().role, ChatRole::User));
    }
}
