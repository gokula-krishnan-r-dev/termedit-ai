//! Structured model output.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantPlan {
    pub explanation: String,
    #[serde(default)]
    pub suggested_fixes: Vec<String>,
    #[serde(default)]
    pub shell_commands: Vec<String>,
    #[serde(default)]
    pub file_edits: Vec<FileEdit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileEdit {
    pub path: String,
    #[serde(default)]
    pub unified_diff: Option<String>,
    #[serde(default)]
    pub new_content: Option<String>,
}

impl AssistantPlan {
    pub fn parse_model_text(raw: &str) -> anyhow::Result<Self> {
        let trimmed = raw.trim();
        let json_str = strip_markdown_fences(trimmed);
        serde_json::from_str(json_str).map_err(Into::into)
    }
}

fn strip_markdown_fences(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json") {
        return rest.trim_end_matches('`').trim();
    }
    if let Some(rest) = s.strip_prefix("```") {
        return rest.trim_end_matches('`').trim();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json() {
        let j = r#"{"explanation":"ok","suggested_fixes":[],"shell_commands":[],"file_edits":[]}"#;
        let p = AssistantPlan::parse_model_text(j).unwrap();
        assert_eq!(p.explanation, "ok");
    }
}
