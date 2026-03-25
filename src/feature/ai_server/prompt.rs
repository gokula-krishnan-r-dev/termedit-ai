//! Token-conscious prompts for server context mode.

use super::context::ServerContext;

pub const MAX_CONTEXT_JSON_BYTES: usize = 120_000;

pub fn system_instruction_json_only() -> String {
    r#"You are an expert DevOps assistant. The user message contains a JSON object "server_context" describing logs, configs, metrics, and environment keys (values may be REDACTED).

You MUST respond with a single JSON object only (no markdown fences, no prose outside JSON) with this exact shape:
{
  "explanation": "string — clear diagnosis or answer for the user",
  "suggested_fixes": ["string", "..."],
  "shell_commands": ["optional safe commands the user may run manually; never destructive without warning"],
  "file_edits": [
    {
      "path": "absolute or repo-relative path",
      "unified_diff": "optional unified diff as a string",
      "new_content": "optional full new file contents if no diff"
    }
  ]
}

Rules:
- Prefer actionable, ordered steps.
- If data is missing (permissions), say so in explanation.
- Do not invent file contents; only suggest edits grounded in context.
- Keep strings concise; avoid huge file_edits."#
        .to_string()
}

pub fn user_message(query: &str, ctx: &ServerContext) -> anyhow::Result<String> {
    let ctx_val = serde_json::to_value(ctx)?;
    let mut wrapper = serde_json::json!({
        "user_query": query,
        "server_context": ctx_val,
    });
    let mut s = wrapper.to_string();
    if s.len() > MAX_CONTEXT_JSON_BYTES {
        // Trim context: drop largest log texts first
        if let Some(ctx_val) = wrapper.get_mut("server_context") {
            if let Some(logs) = ctx_val.get_mut("logs").and_then(|l| l.as_array_mut()) {
                for log in logs.iter_mut() {
                    if let Some(t) = log.get_mut("text").and_then(|x| x.as_str()) {
                        let keep = (MAX_CONTEXT_JSON_BYTES / 8).max(4096);
                        if t.len() > keep {
                            let tail = t.chars().rev().take(keep).collect::<String>();
                            let tail: String = tail.chars().rev().collect();
                            *log.get_mut("text").unwrap() = serde_json::Value::String(format!(
                                "[truncated for token limit]\n{}",
                                tail
                            ));
                        }
                    }
                }
            }
        }
        s = wrapper.to_string();
        if s.len() > MAX_CONTEXT_JSON_BYTES {
            anyhow::bail!(
                "context still too large ({} bytes); narrow log roots or increase limits in code",
                s.len()
            );
        }
    }
    Ok(s)
}
