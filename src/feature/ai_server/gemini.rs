//! Async Gemini `generateContent` for `--ai-server`.

use anyhow::Result;
use reqwest::Client;
use serde::Serialize;

use crate::feature::gemini_chat::{extract_response_text, GeminiError};

const API_TIMEOUT_SECS: u64 = 120;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction<'a>>,
    contents: Vec<Content<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    response_mime_type: &'static str,
}

#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate_json(
        &self,
        system_instruction: &str,
        user_text: &str,
    ) -> Result<String>;
}

pub struct GeminiLlm {
    client: Client,
    api_key: String,
    model_id: String,
}

impl GeminiLlm {
    pub fn new(api_key: String, model_id: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .build()?;
        Ok(Self {
            client,
            api_key,
            model_id,
        })
    }
}

#[async_trait::async_trait]
impl LlmClient for GeminiLlm {
    async fn generate_json(
        &self,
        system_instruction: &str,
        user_text: &str,
    ) -> Result<String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model_id, self.api_key
        );
        let body = GenerateContentBody {
            system_instruction: Some(SystemInstruction {
                parts: [Part {
                    text: system_instruction,
                }],
            }),
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: user_text }],
            }],
            generation_config: Some(GenerationConfig {
                response_mime_type: "application/json",
            }),
        };
        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            let err = extract_response_text(&text).unwrap_or_else(|_| text.clone());
            return Err(anyhow::anyhow!("Gemini HTTP {}: {}", status, err));
        }
        extract_response_text(&text).map_err(|e: GeminiError| anyhow::anyhow!("{}", e))
    }
}

/// Test double: returns fixed JSON text.
pub struct MockLlm {
    pub response: String,
}

#[async_trait::async_trait]
impl LlmClient for MockLlm {
    async fn generate_json(
        &self,
        _system_instruction: &str,
        _user_text: &str,
    ) -> Result<String> {
        Ok(self.response.clone())
    }
}
