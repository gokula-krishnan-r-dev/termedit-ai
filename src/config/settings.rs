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
    /// Enable bracket matching highlight.
    pub bracket_matching: bool,
    /// Enable auto-indent.
    pub auto_indent: bool,
    /// Maximum line length hint (ruler).
    pub ruler: Option<usize>,
    /// Enable minimap.
    pub minimap: bool,
    /// Restore previous session (open files, cursor, scroll) on startup when no files given.
    pub session_restore: bool,
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
            auto_save: 0,
            show_whitespace: false,
            smooth_scroll: true,
            cursor_blink_ms: 500,
            scroll_padding: 5,
            font_size: 14,
            bracket_matching: true,
            auto_indent: true,
            ruler: None,
            minimap: false,
            session_restore: true,
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
    pub fn merge_cli(&mut self, theme: Option<&str>, no_ai: bool, no_restore: bool) {
        if let Some(theme_name) = theme {
            self.theme = theme_name.to_string();
        }
        if no_ai {
            self.ai_enabled = false;
        }
        if no_restore {
            self.session_restore = false;
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
        s.merge_cli(Some("tokyo-night"), true, false);
        assert_eq!(s.theme, "tokyo-night");
        assert!(!s.ai_enabled);
        assert!(s.session_restore);
        s.merge_cli(None, false, true);
        assert!(!s.session_restore);
    }
}
