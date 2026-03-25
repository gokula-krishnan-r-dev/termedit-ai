/// Theme loading and color management.
///
/// Themes are defined in TOML files and can be bundled or user-defined.
/// Each theme provides colors for editor, syntax, UI, and git elements.

use ratatui::style::Color;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::error::{Result, TermEditError};

/// Complete theme definition.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub editor: EditorColors,
    pub syntax: HashMap<String, Color>,
    pub ui: UiColors,
    pub git: GitColors,
}

/// Editor area colors.
#[derive(Debug, Clone)]
pub struct EditorColors {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection_bg: Color,
    pub line_number: Color,
    pub line_number_active: Color,
    pub gutter_bg: Color,
    pub current_line_bg: Color,
}

/// UI element colors.
#[derive(Debug, Clone)]
pub struct UiColors {
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub tab_active_bg: Color,
    pub tab_inactive_bg: Color,
    pub tab_fg: Color,
    pub panel_border: Color,
    pub ai_ghost_text: Color,
    pub ai_response_bg: Color,
    pub search_match_bg: Color,
    pub search_current_bg: Color,
}

/// Git status colors.
#[derive(Debug, Clone)]
pub struct GitColors {
    pub added: Color,
    pub modified: Color,
    pub deleted: Color,
}

/// TOML deserialization structures.
#[derive(Deserialize)]
struct ThemeToml {
    editor: Option<EditorToml>,
    syntax: Option<HashMap<String, String>>,
    ui: Option<UiToml>,
    git: Option<GitToml>,
}

#[derive(Deserialize)]
struct EditorToml {
    background: Option<String>,
    foreground: Option<String>,
    cursor: Option<String>,
    selection_bg: Option<String>,
    line_number: Option<String>,
    line_number_active: Option<String>,
    gutter_bg: Option<String>,
    current_line_bg: Option<String>,
}

#[derive(Deserialize)]
struct UiToml {
    status_bar_bg: Option<String>,
    status_bar_fg: Option<String>,
    tab_active_bg: Option<String>,
    tab_inactive_bg: Option<String>,
    tab_fg: Option<String>,
    panel_border: Option<String>,
    ai_ghost_text: Option<String>,
    ai_response_bg: Option<String>,
    search_match_bg: Option<String>,
    search_current_bg: Option<String>,
}

#[derive(Deserialize)]
struct GitToml {
    added: Option<String>,
    modified: Option<String>,
    deleted: Option<String>,
}

/// Parse a hex color string (#RRGGBB) to ratatui Color.
pub fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::Reset;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    Color::Rgb(r, g, b)
}

fn color_or(opt: &Option<String>, default: Color) -> Color {
    opt.as_ref().map(|s| hex_to_color(s)).unwrap_or(default)
}

impl Theme {
    /// Load a theme from a TOML file path.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(TermEditError::Io)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom")
            .to_string();
        Self::from_toml(&content, &name)
    }

    /// Parse a theme from TOML string content.
    fn from_toml(content: &str, name: &str) -> Result<Self> {
        let parsed: ThemeToml = toml::from_str(content)?;
        Ok(Self::from_parsed(parsed, name))
    }

    /// Build a Theme from parsed TOML.
    fn from_parsed(parsed: ThemeToml, name: &str) -> Self {
        let editor_toml = parsed.editor.unwrap_or(EditorToml {
            background: None,
            foreground: None,
            cursor: None,
            selection_bg: None,
            line_number: None,
            line_number_active: None,
            gutter_bg: None,
            current_line_bg: None,
        });

        let ui_toml = parsed.ui.unwrap_or(UiToml {
            status_bar_bg: None,
            status_bar_fg: None,
            tab_active_bg: None,
            tab_inactive_bg: None,
            tab_fg: None,
            panel_border: None,
            ai_ghost_text: None,
            ai_response_bg: None,
            search_match_bg: None,
            search_current_bg: None,
        });

        let git_toml = parsed.git.unwrap_or(GitToml {
            added: None,
            modified: None,
            deleted: None,
        });

        let mut syntax_colors = HashMap::new();
        if let Some(syntax) = parsed.syntax {
            for (key, value) in syntax {
                syntax_colors.insert(key, hex_to_color(&value));
            }
        }

        // Apply defaults for any missing syntax colors
        let defaults = Self::dark_plus();
        for (key, color) in &defaults.syntax {
            syntax_colors.entry(key.clone()).or_insert(*color);
        }

        Self {
            name: name.to_string(),
            editor: EditorColors {
                background: color_or(&editor_toml.background, Color::Rgb(30, 30, 30)),
                foreground: color_or(&editor_toml.foreground, Color::Rgb(212, 212, 212)),
                cursor: color_or(&editor_toml.cursor, Color::Rgb(174, 175, 173)),
                selection_bg: color_or(&editor_toml.selection_bg, Color::Rgb(38, 79, 120)),
                line_number: color_or(&editor_toml.line_number, Color::Rgb(133, 133, 133)),
                line_number_active: color_or(
                    &editor_toml.line_number_active,
                    Color::Rgb(198, 198, 198),
                ),
                gutter_bg: color_or(&editor_toml.gutter_bg, Color::Rgb(30, 30, 30)),
                current_line_bg: color_or(
                    &editor_toml.current_line_bg,
                    Color::Rgb(42, 45, 46),
                ),
            },
            syntax: syntax_colors,
            ui: UiColors {
                status_bar_bg: color_or(&ui_toml.status_bar_bg, Color::Rgb(0, 122, 204)),
                status_bar_fg: color_or(&ui_toml.status_bar_fg, Color::Rgb(255, 255, 255)),
                tab_active_bg: color_or(&ui_toml.tab_active_bg, Color::Rgb(30, 30, 30)),
                tab_inactive_bg: color_or(&ui_toml.tab_inactive_bg, Color::Rgb(45, 45, 45)),
                tab_fg: color_or(&ui_toml.tab_fg, Color::Rgb(204, 204, 204)),
                panel_border: color_or(&ui_toml.panel_border, Color::Rgb(69, 69, 69)),
                ai_ghost_text: color_or(&ui_toml.ai_ghost_text, Color::Rgb(115, 115, 115)),
                ai_response_bg: color_or(&ui_toml.ai_response_bg, Color::Rgb(37, 37, 38)),
                search_match_bg: color_or(
                    &ui_toml.search_match_bg,
                    Color::Rgb(81, 69, 16),
                ),
                search_current_bg: color_or(
                    &ui_toml.search_current_bg,
                    Color::Rgb(120, 100, 20),
                ),
            },
            git: GitColors {
                added: color_or(&git_toml.added, Color::Rgb(78, 201, 176)),
                modified: color_or(&git_toml.modified, Color::Rgb(86, 156, 214)),
                deleted: color_or(&git_toml.deleted, Color::Rgb(244, 135, 113)),
            },
        }
    }

    /// VS Code Dark+ theme (default).
    pub fn dark_plus() -> Self {
        let mut syntax = HashMap::new();
        syntax.insert("keyword".to_string(), Color::Rgb(86, 156, 214));
        syntax.insert("string".to_string(), Color::Rgb(206, 145, 120));
        syntax.insert("number".to_string(), Color::Rgb(181, 206, 168));
        syntax.insert("comment".to_string(), Color::Rgb(106, 153, 85));
        syntax.insert("function".to_string(), Color::Rgb(220, 220, 170));
        syntax.insert("type_name".to_string(), Color::Rgb(78, 201, 176));
        syntax.insert("variable".to_string(), Color::Rgb(156, 220, 254));
        syntax.insert("operator".to_string(), Color::Rgb(212, 212, 212));
        syntax.insert("constant".to_string(), Color::Rgb(79, 193, 255));
        syntax.insert("attribute".to_string(), Color::Rgb(156, 220, 254));
        syntax.insert("punctuation".to_string(), Color::Rgb(212, 212, 212));

        Self {
            name: "dark-plus".to_string(),
            editor: EditorColors {
                background: Color::Rgb(30, 30, 30),
                foreground: Color::Rgb(212, 212, 212),
                cursor: Color::Rgb(174, 175, 173),
                selection_bg: Color::Rgb(38, 79, 120),
                line_number: Color::Rgb(133, 133, 133),
                line_number_active: Color::Rgb(198, 198, 198),
                gutter_bg: Color::Rgb(30, 30, 30),
                current_line_bg: Color::Rgb(42, 45, 46),
            },
            syntax,
            ui: UiColors {
                status_bar_bg: Color::Rgb(0, 122, 204),
                status_bar_fg: Color::Rgb(255, 255, 255),
                tab_active_bg: Color::Rgb(30, 30, 30),
                tab_inactive_bg: Color::Rgb(45, 45, 45),
                tab_fg: Color::Rgb(204, 204, 204),
                panel_border: Color::Rgb(69, 69, 69),
                ai_ghost_text: Color::Rgb(115, 115, 115),
                ai_response_bg: Color::Rgb(37, 37, 38),
                search_match_bg: Color::Rgb(81, 69, 16),
                search_current_bg: Color::Rgb(120, 100, 20),
            },
            git: GitColors {
                added: Color::Rgb(78, 201, 176),
                modified: Color::Rgb(86, 156, 214),
                deleted: Color::Rgb(244, 135, 113),
            },
        }
    }

    /// One Dark Pro theme.
    pub fn one_dark_pro() -> Self {
        let mut syntax = HashMap::new();
        syntax.insert("keyword".to_string(), Color::Rgb(198, 120, 221));
        syntax.insert("string".to_string(), Color::Rgb(152, 195, 121));
        syntax.insert("number".to_string(), Color::Rgb(209, 154, 102));
        syntax.insert("comment".to_string(), Color::Rgb(92, 99, 112));
        syntax.insert("function".to_string(), Color::Rgb(97, 175, 239));
        syntax.insert("type_name".to_string(), Color::Rgb(229, 192, 123));
        syntax.insert("variable".to_string(), Color::Rgb(224, 108, 117));
        syntax.insert("operator".to_string(), Color::Rgb(171, 178, 191));
        syntax.insert("constant".to_string(), Color::Rgb(209, 154, 102));
        syntax.insert("attribute".to_string(), Color::Rgb(209, 154, 102));
        syntax.insert("punctuation".to_string(), Color::Rgb(171, 178, 191));

        Self {
            name: "one-dark-pro".to_string(),
            editor: EditorColors {
                background: Color::Rgb(40, 44, 52),
                foreground: Color::Rgb(171, 178, 191),
                cursor: Color::Rgb(82, 139, 255),
                selection_bg: Color::Rgb(62, 68, 81),
                line_number: Color::Rgb(76, 82, 99),
                line_number_active: Color::Rgb(171, 178, 191),
                gutter_bg: Color::Rgb(40, 44, 52),
                current_line_bg: Color::Rgb(44, 49, 58),
            },
            syntax,
            ui: UiColors {
                status_bar_bg: Color::Rgb(33, 37, 43),
                status_bar_fg: Color::Rgb(157, 165, 180),
                tab_active_bg: Color::Rgb(40, 44, 52),
                tab_inactive_bg: Color::Rgb(33, 37, 43),
                tab_fg: Color::Rgb(157, 165, 180),
                panel_border: Color::Rgb(24, 26, 31),
                ai_ghost_text: Color::Rgb(76, 82, 99),
                ai_response_bg: Color::Rgb(33, 37, 43),
                search_match_bg: Color::Rgb(72, 56, 0),
                search_current_bg: Color::Rgb(100, 80, 10),
            },
            git: GitColors {
                added: Color::Rgb(152, 195, 121),
                modified: Color::Rgb(97, 175, 239),
                deleted: Color::Rgb(224, 108, 117),
            },
        }
    }

    /// Catppuccin Mocha theme.
    pub fn catppuccin_mocha() -> Self {
        let mut syntax = HashMap::new();
        syntax.insert("keyword".to_string(), Color::Rgb(203, 166, 247));
        syntax.insert("string".to_string(), Color::Rgb(166, 227, 161));
        syntax.insert("number".to_string(), Color::Rgb(250, 179, 135));
        syntax.insert("comment".to_string(), Color::Rgb(108, 112, 134));
        syntax.insert("function".to_string(), Color::Rgb(137, 180, 250));
        syntax.insert("type_name".to_string(), Color::Rgb(249, 226, 175));
        syntax.insert("variable".to_string(), Color::Rgb(205, 214, 244));
        syntax.insert("operator".to_string(), Color::Rgb(137, 220, 235));
        syntax.insert("constant".to_string(), Color::Rgb(250, 179, 135));
        syntax.insert("attribute".to_string(), Color::Rgb(249, 226, 175));
        syntax.insert("punctuation".to_string(), Color::Rgb(147, 153, 178));

        Self {
            name: "catppuccin-mocha".to_string(),
            editor: EditorColors {
                background: Color::Rgb(30, 30, 46),
                foreground: Color::Rgb(205, 214, 244),
                cursor: Color::Rgb(245, 224, 220),
                selection_bg: Color::Rgb(69, 71, 90),
                line_number: Color::Rgb(88, 91, 112),
                line_number_active: Color::Rgb(166, 173, 200),
                gutter_bg: Color::Rgb(30, 30, 46),
                current_line_bg: Color::Rgb(49, 50, 68),
            },
            syntax,
            ui: UiColors {
                status_bar_bg: Color::Rgb(24, 24, 37),
                status_bar_fg: Color::Rgb(186, 194, 222),
                tab_active_bg: Color::Rgb(30, 30, 46),
                tab_inactive_bg: Color::Rgb(24, 24, 37),
                tab_fg: Color::Rgb(166, 173, 200),
                panel_border: Color::Rgb(69, 71, 90),
                ai_ghost_text: Color::Rgb(88, 91, 112),
                ai_response_bg: Color::Rgb(24, 24, 37),
                search_match_bg: Color::Rgb(69, 60, 20),
                search_current_bg: Color::Rgb(100, 90, 30),
            },
            git: GitColors {
                added: Color::Rgb(166, 227, 161),
                modified: Color::Rgb(137, 180, 250),
                deleted: Color::Rgb(243, 139, 168),
            },
        }
    }

    /// Get a built-in theme by name.
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "dark-plus" => Some(Self::dark_plus()),
            "one-dark-pro" => Some(Self::one_dark_pro()),
            "catppuccin-mocha" => Some(Self::catppuccin_mocha()),
            _ => None,
        }
    }

    /// Load a theme by name — checks user dir, then built-in.
    pub fn load(name: &str) -> Self {
        // Try user theme directory
        if let Some(config_dir) = dirs::config_dir() {
            let theme_path = config_dir
                .join("termedit")
                .join("themes")
                .join(format!("{}.toml", name));
            if theme_path.exists() {
                if let Ok(theme) = Self::from_file(&theme_path) {
                    return theme;
                }
            }
        }

        // Fall back to built-in
        Self::builtin(name).unwrap_or_else(Self::dark_plus)
    }

    /// Get a syntax color by highlight group name.
    pub fn syntax_color(&self, group: &str) -> Color {
        self.syntax
            .get(group)
            .copied()
            .unwrap_or(self.editor.foreground)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_color() {
        assert_eq!(hex_to_color("#FF0000"), Color::Rgb(255, 0, 0));
        assert_eq!(hex_to_color("#00FF00"), Color::Rgb(0, 255, 0));
        assert_eq!(hex_to_color("#0000FF"), Color::Rgb(0, 0, 255));
        assert_eq!(hex_to_color("#1E1E1E"), Color::Rgb(30, 30, 30));
    }

    #[test]
    fn test_dark_plus_theme() {
        let theme = Theme::dark_plus();
        assert_eq!(theme.name, "dark-plus");
        assert!(theme.syntax.contains_key("keyword"));
        assert!(theme.syntax.contains_key("string"));
    }

    #[test]
    fn test_builtin_themes() {
        assert!(Theme::builtin("dark-plus").is_some());
        assert!(Theme::builtin("one-dark-pro").is_some());
        assert!(Theme::builtin("catppuccin-mocha").is_some());
        assert!(Theme::builtin("nonexistent").is_none());
    }

    #[test]
    fn test_parse_theme_toml() {
        let toml = r##"
[editor]
background = "#282C34"
foreground = "#ABB2BF"

[syntax]
keyword = "#C678DD"
string = "#98C379"

[ui]
status_bar_bg = "#21252B"
status_bar_fg = "#9DA5B4"

[git]
added = "#98C379"
"##;
        let theme = Theme::from_toml(toml, "test").unwrap();
        assert_eq!(theme.editor.background, Color::Rgb(40, 44, 52));
        assert_eq!(
            theme.syntax.get("keyword").copied(),
            Some(Color::Rgb(198, 120, 221))
        );
    }
}
