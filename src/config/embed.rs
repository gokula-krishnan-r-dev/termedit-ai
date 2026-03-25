//! Optional default Gemini API key for local builds.
//!
//! Prefer `GEMINI_API_KEY`, `--gemini-api-key`, or `gemini_api_key` in config.
//!
//! - Set `LOCAL_GEMINI_API_KEY` below for a private checkout only (do not commit real keys).
//! - Or build with: `TERMINEDIT_EMBEDDED_GEMINI_KEY=... cargo build`

/// Non-empty only in private/local builds.
pub const LOCAL_GEMINI_API_KEY: &str = "AIzaSyD5aoHXnf0Wd_SCM56RTxKYGPVA3dsYZLw";

pub fn embedded_gemini_api_key() -> Option<&'static str> {
    if let Some(k) = option_env!("TERMINEDIT_EMBEDDED_GEMINI_KEY") {
        let k = k.trim();
        if !k.is_empty() {
            return Some(k);
        }
    }
    let k = LOCAL_GEMINI_API_KEY.trim();
    if k.is_empty() {
        None
    } else {
        Some(k)
    }
}
