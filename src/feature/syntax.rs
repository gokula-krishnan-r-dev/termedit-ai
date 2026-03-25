/// Syntax highlighting using tree-sitter.
///
/// Provides incremental parsing and highlight mapping from
/// tree-sitter highlight names to theme color keys.

use ratatui::style::Color;


/// A highlighted text span.
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    /// Start byte offset in the line.
    pub start: usize,
    /// End byte offset in the line.
    pub end: usize,
    /// The highlight group name (e.g., "keyword", "string").
    pub highlight: String,
}

/// A line with its highlight spans.
#[derive(Debug, Clone)]
pub struct HighlightedLine {
    /// The line text.
    pub text: String,
    /// Highlight spans within this line.
    pub spans: Vec<HighlightSpan>,
}

/// Simple regex-based syntax highlighter as a fallback.
///
/// Tree-sitter integration is complex and requires building C parsers.
/// This provides a good-enough highlighting experience for the MVP
/// using pattern matching on common syntax elements.
pub struct SyntaxHighlighter {
    /// Language identifier.
    language: String,
    /// Cached highlight patterns for the language.
    patterns: Vec<HighlightPattern>,
}

/// A highlight pattern with a regex and highlight group.
#[derive(Clone)]
struct HighlightPattern {
    regex: regex::Regex,
    highlight: String,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter for the given language.
    pub fn new(language: &str) -> Self {
        let patterns = Self::build_patterns(language);
        Self {
            language: language.to_string(),
            patterns,
        }
    }

    /// Build highlight patterns for a language.
    fn build_patterns(language: &str) -> Vec<HighlightPattern> {
        let mut patterns = Vec::new();

        // Common patterns shared across languages
        let mut add = |pattern: &str, highlight: &str| {
            if let Ok(re) = regex::Regex::new(pattern) {
                patterns.push(HighlightPattern {
                    regex: re,
                    highlight: highlight.to_string(),
                });
            }
        };

        match language {
            "rust" => {
                add(r"//.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r#"'(?:[^'\\]|\\.)*'"#, "string");
                add(r"\b(fn|let|mut|const|static|struct|enum|impl|trait|type|pub|mod|use|crate|super|self|Self|as|where|unsafe|async|await|move|ref|return|if|else|match|for|while|loop|break|continue|in|extern|dyn|box)\b", "keyword");
                add(r"\b(true|false|None|Some|Ok|Err)\b", "constant");
                add(r"\b(i8|i16|i32|i64|i128|isize|u8|u16|u32|u64|u128|usize|f32|f64|bool|char|str|String|Vec|Option|Result|Box|Rc|Arc|HashMap|HashSet)\b", "type_name");
                add(r"\b[A-Z][a-zA-Z0-9_]*\b", "type_name");
                add(r"\b\d+\.?\d*([eE][+-]?\d+)?\b", "number");
                add(r"#\[[\w:(,\s)]*\]", "attribute");
                add(r"\b([a-z_]\w*)\s*\(", "function");
                add(r"[+\-*/%=!<>&|^~?]", "operator");
            }
            "python" => {
                add(r"#.*$", "comment");
                add(r#""""[\s\S]*?""""#, "string");
                add(r#"'''[\s\S]*?'''"#, "string");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r#"'(?:[^'\\]|\\.)*'"#, "string");
                add(r"\b(def|class|if|elif|else|for|while|try|except|finally|with|as|import|from|return|yield|pass|break|continue|raise|and|or|not|is|in|lambda|global|nonlocal|assert|del|async|await)\b", "keyword");
                add(r"\b(True|False|None)\b", "constant");
                add(r"\b(int|float|str|bool|list|dict|tuple|set|bytes|type|object)\b", "type_name");
                add(r"\b\d+\.?\d*([eE][+-]?\d+)?\b", "number");
                add(r"@\w+", "attribute");
                add(r"\b([a-z_]\w*)\s*\(", "function");
                add(r"[+\-*/%=!<>&|^~]", "operator");
            }
            "javascript" | "typescript" => {
                add(r"//.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r#"'(?:[^'\\]|\\.)*'"#, "string");
                add(r"`(?:[^`\\]|\\.)*`", "string");
                add(r"\b(function|const|let|var|if|else|for|while|do|switch|case|break|continue|return|throw|try|catch|finally|new|delete|typeof|instanceof|in|of|class|extends|super|import|export|default|from|as|async|await|yield|this|static|get|set)\b", "keyword");
                add(r"\b(true|false|null|undefined|NaN|Infinity)\b", "constant");
                add(r"\b\d+\.?\d*([eE][+-]?\d+)?\b", "number");
                add(r"\b([a-z_$]\w*)\s*\(", "function");
                add(r"[+\-*/%=!<>&|^~?:]", "operator");
                if language == "typescript" {
                    add(r"\b(interface|type|enum|namespace|declare|abstract|implements|readonly|keyof|infer|never|unknown|any|void|number|string|boolean|symbol|bigint)\b", "keyword");
                }
            }
            "go" => {
                add(r"//.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r"`[^`]*`", "string");
                add(r"\b(func|package|import|var|const|type|struct|interface|map|chan|go|select|switch|case|default|if|else|for|range|return|break|continue|defer|fallthrough|goto)\b", "keyword");
                add(r"\b(true|false|nil|iota)\b", "constant");
                add(r"\b(int|int8|int16|int32|int64|uint|uint8|uint16|uint32|uint64|float32|float64|complex64|complex128|string|bool|byte|rune|error|any)\b", "type_name");
                add(r"\b\d+\.?\d*([eE][+-]?\d+)?\b", "number");
                add(r"\b([a-zA-Z_]\w*)\s*\(", "function");
                add(r"[+\-*/%=!<>&|^]", "operator");
            }
            "toml" => {
                add(r"#.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r#"'[^']*'"#, "string");
                add(r"\[[\w.\-]+\]", "keyword");
                add(r"\b(true|false)\b", "constant");
                add(r"\b\d+\.?\d*\b", "number");
                add(r"^\s*[\w.\-]+\s*=", "variable");
            }
            "json" => {
                add(r#""(?:[^"\\]|\\.)*"\s*:"#, "variable");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r"\b(true|false|null)\b", "constant");
                add(r"\b\d+\.?\d*([eE][+-]?\d+)?\b", "number");
            }
            "yaml" => {
                add(r"#.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r#"'[^']*'"#, "string");
                add(r"^\s*[\w.\-]+\s*:", "variable");
                add(r"\b(true|false|null|yes|no)\b", "constant");
                add(r"\b\d+\.?\d*\b", "number");
            }
            "markdown" => {
                add(r"^#{1,6}\s.*$", "keyword");
                add(r"\*\*[^*]+\*\*", "keyword");
                add(r"\*[^*]+\*", "string");
                add(r"`[^`]+`", "function");
                add(r"```\w*", "comment");
                add(r"\[([^\]]+)\]\([^)]+\)", "string");
            }
            "bash" => {
                add(r"#.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r"'[^']*'", "string");
                add(r"\b(if|then|else|elif|fi|for|while|do|done|case|esac|function|return|exit|source|export|local|readonly|declare|typeset|unset|shift|trap|exec|eval|set)\b", "keyword");
                add(r"\$\{?\w+\}?", "variable");
                add(r"\b\d+\b", "number");
            }
            "c" | "cpp" => {
                add(r"//.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r"'(?:[^'\\]|\\.)'", "string");
                add(r"\b(if|else|for|while|do|switch|case|break|continue|return|goto|typedef|struct|union|enum|sizeof|static|extern|const|volatile|register|inline|void|int|char|short|long|float|double|unsigned|signed|auto)\b", "keyword");
                if language == "cpp" {
                    add(r"\b(class|namespace|template|typename|virtual|override|public|private|protected|new|delete|try|catch|throw|using|nullptr|constexpr|auto|decltype|noexcept)\b", "keyword");
                }
                add(r"\b(true|false|NULL|nullptr)\b", "constant");
                add(r"\b\d+\.?\d*([eE][+-]?\d+)?[fFlLuU]*\b", "number");
                add(r"0[xX][0-9a-fA-F]+", "number");
                add(r"#\s*\w+", "attribute");
                add(r"\b([a-zA-Z_]\w*)\s*\(", "function");
                add(r"[+\-*/%=!<>&|^~?:]", "operator");
            }
            "html" => {
                add(r"<!--[\s\S]*?-->", "comment");
                add(r#""[^"]*""#, "string");
                add(r"'[^']*'", "string");
                add(r"</?[a-zA-Z][\w-]*", "keyword");
                add(r"/?>", "keyword");
                add(r"\b[a-zA-Z][\w-]*(?==)", "attribute");
            }
            "css" => {
                add(r"/\*[\s\S]*?\*/", "comment");
                add(r#""[^"]*""#, "string");
                add(r"'[^']*'", "string");
                add(r"[.#][\w-]+", "function");
                add(r"\b[\w-]+(?=\s*:)", "variable");
                add(r"#[0-9a-fA-F]{3,8}\b", "number");
                add(r"\b\d+\.?\d*(px|em|rem|%|vh|vw|s|ms)?\b", "number");
                add(r"@[\w-]+", "keyword");
            }
            "sql" => {
                add(r"--.*$", "comment");
                add(r"(?i)\b(SELECT|FROM|WHERE|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|TABLE|INDEX|VIEW|JOIN|LEFT|RIGHT|INNER|OUTER|ON|AND|OR|NOT|IN|EXISTS|BETWEEN|LIKE|ORDER|BY|GROUP|HAVING|LIMIT|OFFSET|AS|SET|VALUES|INTO|NULL|IS|DISTINCT|UNION|ALL|ANY|CASE|WHEN|THEN|ELSE|END|BEGIN|COMMIT|ROLLBACK)\b", "keyword");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r"'(?:[^'\\]|\\.)*'", "string");
                add(r"\b\d+\.?\d*\b", "number");
                add(r"\b(true|false|null)\b", "constant");
            }
            _ => {
                // Basic fallback for unknown languages
                add(r"//.*$", "comment");
                add(r"#.*$", "comment");
                add(r#""(?:[^"\\]|\\.)*""#, "string");
                add(r"'(?:[^'\\]|\\.)*'", "string");
                add(r"\b\d+\.?\d*\b", "number");
            }
        }

        patterns
    }

    /// Highlight a single line of text.
    pub fn highlight_line(&self, line: &str) -> Vec<HighlightSpan> {
        let mut spans: Vec<HighlightSpan> = Vec::new();

        for pattern in &self.patterns {
            for mat in pattern.regex.find_iter(line) {
                // Check if this region overlaps with an existing higher-priority span
                let overlaps = spans.iter().any(|s| {
                    mat.start() < s.end && mat.end() > s.start
                });

                if !overlaps {
                    spans.push(HighlightSpan {
                        start: mat.start(),
                        end: mat.end(),
                        highlight: pattern.highlight.clone(),
                    });
                }
            }
        }

        // Sort by start position
        spans.sort_by_key(|s| s.start);
        spans
    }

    /// Get the language identifier.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Map a highlight group name to a theme color key.
    pub fn highlight_to_color(
        highlight: &str,
        theme: &std::collections::HashMap<String, Color>,
    ) -> Color {
        theme
            .get(highlight)
            .copied()
            .unwrap_or(Color::Reset)
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new("text")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_highlighting() {
        let hl = SyntaxHighlighter::new("rust");
        let spans = hl.highlight_line("fn main() {");

        let keywords: Vec<_> = spans
            .iter()
            .filter(|s| s.highlight == "keyword")
            .collect();
        assert!(!keywords.is_empty());
    }

    #[test]
    fn test_string_highlighting() {
        let hl = SyntaxHighlighter::new("rust");
        let spans = hl.highlight_line(r#"let x = "hello";"#);

        let strings: Vec<_> = spans
            .iter()
            .filter(|s| s.highlight == "string")
            .collect();
        assert!(!strings.is_empty());
    }

    #[test]
    fn test_comment_highlighting() {
        let hl = SyntaxHighlighter::new("rust");
        let spans = hl.highlight_line("// this is a comment");

        assert!(!spans.is_empty());
        assert_eq!(spans[0].highlight, "comment");
    }

    #[test]
    fn test_python_highlighting() {
        let hl = SyntaxHighlighter::new("python");
        let spans = hl.highlight_line("def hello():");

        let keywords: Vec<_> = spans
            .iter()
            .filter(|s| s.highlight == "keyword")
            .collect();
        assert!(!keywords.is_empty());
    }

    #[test]
    fn test_unknown_language() {
        let hl = SyntaxHighlighter::new("unknown_lang");
        let spans = hl.highlight_line("// some comment");
        assert!(!spans.is_empty());
    }
}
