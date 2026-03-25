/// Editor settings loaded from config file and CLI flags.

use serde::Deserialize;
use std::path::Path;

use crate::error::Result;

/// All configurable editor settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Tab width in spaces.
    pub tab_size: usize,
    /// Use spaces instead of tabs.
    pub soft_tabs: bool,
    /// Enable word wrap.
    pub word_wrap: bool,
    /// Show line numbers.
    pub line_numbers: bool,
    /// Enable mouse support.
    pub mouse: bool,
    /// Theme name.
    pub theme: String,
    /// Enable AI features.
    pub ai_enabled: bool,
    /// AI completion debounce in ms (default 200).
    pub ai_debounce_ms: u64,
    /// Optional AI model override (e.g. faster/smaller); None = default.
    pub ai_model: Option<String>,
    /// Gemini model id for the AI assistant panel (`GEMINI_CHAT_MODELS`); None = first listed.
    pub ai_chat_model: Option<String>,
    /// Optional Gemini API key; env `GEMINI_API_KEY` is preferred when set.
    pub gemini_api_key: Option<String>,
    /// Enable auto-save (in seconds, 0 = disabled).
    pub auto_save: u64,
    /// Show whitespace characters.
    pub show_whitespace: bool,
    /// Enable smooth scrolling.
    pub smooth_scroll: bool,
    /// Cursor blink rate (ms, 0 = no blink).
    pub cursor_blink_ms: u64,
    /// Scroll padding (lines of context above/below cursor).
    pub scroll_padding: usize,
    /// Font size (for terminal reference).
    pub font_size: u16,
    /// Enable bracket matching highlight (paired `()`, `[]`, `{}`).
    pub bracket_matching: bool,
    /// Max character count to scan for bracket matching; larger buffers skip highlight and jump.
    pub bracket_match_max_chars: usize,
    /// Enable auto-indent.
    pub auto_indent: bool,
    /// Maximum line length hint (ruler).
    pub ruler: Option<usize>,
    /// Enable minimap.
    pub minimap: bool,
    /// Restore previous session (open files, cursor, scroll) on startup when no files given.
    pub session_restore: bool,
    /// Enable Go to Symbol (outline) / Ctrl+Shift+O.
    pub outline_enabled: bool,
    /// Max UTF-8 bytes to parse for outline; larger buffers skip parsing.
    pub outline_max_bytes: usize,
    /// Enable Find in Open Tabs (Ctrl/Cmd+Shift+F).
    pub find_in_open_tabs_enabled: bool,
    /// Max result rows to collect across all open tabs.
    pub find_in_open_tabs_max_results: usize,
    /// Skip tabs whose character length exceeds this (no search in that buffer).
    pub find_in_open_tabs_max_chars_per_tab: usize,
    /// Debounce before re-running search after query changes (ms).
    pub find_in_open_tabs_debounce_ms: u64,
    /// Case-sensitive search in Find in Open Tabs.
    pub find_in_open_tabs_case_sensitive: bool,
    /// Whole-word only in Find in Open Tabs.
    pub find_in_open_tabs_whole_word: bool,
    /// Use regex in Find in Open Tabs (otherwise literal).
    pub find_in_open_tabs_regex: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tab_size: 4,
            soft_tabs: true,
            word_wrap: false,
            line_numbers: true,
            mouse: true,
            theme: "dark-plus".to_string(),
            ai_enabled: true,
            ai_debounce_ms: 200,
            ai_model: None,
            ai_chat_model: None,
            gemini_api_key: None,
            auto_save: 0,
            show_whitespace: false,
            smooth_scroll: true,
            cursor_blink_ms: 500,
            scroll_padding: 5,
            font_size: 14,
            bracket_matching: true,
            bracket_match_max_chars: 2_000_000,
            auto_indent: true,
            ruler: None,
            minimap: false,
            session_restore: true,
            outline_enabled: true,
            outline_max_bytes: 2_000_000,
            find_in_open_tabs_enabled: true,
            find_in_open_tabs_max_results: 500,
            find_in_open_tabs_max_chars_per_tab: 2_000_000,
            find_in_open_tabs_debounce_ms: 250,
            find_in_open_tabs_case_sensitive: false,
            find_in_open_tabs_whole_word: false,
            find_in_open_tabs_regex: false,
        }
    }
}

impl Settings {
    /// Load settings from a TOML config file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(crate::error::TermEditError::Io)?;
        let settings: Settings = toml::from_str(&content)?;
        Ok(settings)
    }

    /// Load settings from the default config path, falling back to defaults.
    pub fn load() -> Self {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("termedit").join("config.toml");
            if config_path.exists() {
                if let Ok(settings) = Self::from_file(&config_path) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    /// Merge CLI overrides into settings.
    pub fn merge_cli(
        &mut self,
        theme: Option<&str>,
        no_ai: bool,
        no_restore: bool,
        ai_chat_model: Option<&str>,
        gemini_api_key: Option<&str>,
    ) {
        if let Some(theme_name) = theme {
            self.theme = theme_name.to_string();
        }
        if no_ai {
            self.ai_enabled = false;
        }
        if no_restore {
            self.session_restore = false;
        }
        if let Some(m) = ai_chat_model.map(str::trim).filter(|s| !s.is_empty()) {
            self.ai_chat_model = Some(m.to_string());
        }
        if let Some(k) = gemini_api_key.map(str::trim).filter(|s| !s.is_empty()) {
            self.gemini_api_key = Some(k.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = Settings::default();
        assert_eq!(s.tab_size, 4);
        assert!(s.soft_tabs);
        assert!(s.line_numbers);
        assert!(s.mouse);
        assert_eq!(s.theme, "dark-plus");
        assert!(s.session_restore);
        assert!(s.outline_enabled);
        assert_eq!(s.outline_max_bytes, 2_000_000);
        assert_eq!(s.bracket_match_max_chars, 2_000_000);
        assert!(s.find_in_open_tabs_enabled);
        assert_eq!(s.find_in_open_tabs_max_results, 500);
    }

    #[test]
    fn test_parse_toml() {
        let toml = r#"
tab_size = 2
soft_tabs = false
theme = "catppuccin-mocha"
"#;
        let s: Settings = toml::from_str(toml).unwrap();
        assert_eq!(s.tab_size, 2);
        assert!(!s.soft_tabs);
        assert_eq!(s.theme, "catppuccin-mocha");
        // Defaults should fill in unspecified
        assert!(s.line_numbers);
    }

    #[test]
    fn test_merge_cli() {
        let mut s = Settings::default();
        s.merge_cli(Some("tokyo-night"), true, false, None, None);
        assert_eq!(s.theme, "tokyo-night");
        assert!(!s.ai_enabled);
        assert!(s.session_restore);
        s.merge_cli(None, false, true, None, None);
        assert!(!s.session_restore);
        let mut t = Settings::default();
        t.merge_cli(None, false, false, Some("gemini-1.5-pro"), None);
        assert_eq!(t.ai_chat_model.as_deref(), Some("gemini-1.5-pro"));
    }
}
