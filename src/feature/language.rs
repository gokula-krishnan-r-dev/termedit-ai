/// Language detection from file extension, shebang line, or content heuristics.

use std::path::Path;

use crate::core::buffer::Buffer;

/// Detect the language of a file.
///
/// Priority:
/// 1. Shebang line (e.g. `#!/usr/bin/env python3`)
/// 2. File extension
/// 3. Content heuristics
/// 4. Fallback: "text"
pub fn detect_language(path: &Path, buffer: &Buffer) -> String {
    // Try shebang first
    if let Some(lang) = detect_from_shebang(buffer) {
        return lang;
    }

    // Try extension
    if let Some(lang) = detect_from_extension(path) {
        return lang;
    }

    // Fallback
    "text".to_string()
}

/// Detect language from shebang line.
fn detect_from_shebang(buffer: &Buffer) -> Option<String> {
    if buffer.is_empty() {
        return None;
    }

    let first_line = buffer.line_text(0);
    if !first_line.starts_with("#!") {
        return None;
    }

    let shebang = first_line.to_lowercase();

    if shebang.contains("python") {
        Some("python".to_string())
    } else if shebang.contains("node") || shebang.contains("deno") {
        Some("javascript".to_string())
    } else if shebang.contains("bash") || shebang.contains("sh") || shebang.contains("zsh") {
        Some("bash".to_string())
    } else if shebang.contains("ruby") {
        Some("ruby".to_string())
    } else if shebang.contains("perl") {
        Some("perl".to_string())
    } else {
        None
    }
}

/// Detect language from file extension.
fn detect_from_extension(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();

    let lang = match ext.as_str() {
        // Rust
        "rs" => "rust",
        // Python
        "py" | "pyi" | "pyw" => "python",
        // JavaScript
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        // TypeScript
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        // Go
        "go" => "go",
        // C
        "c" | "h" => "c",
        // C++
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => "cpp",
        // Java
        "java" => "java",
        // Shell
        "sh" | "bash" | "zsh" => "bash",
        // Config formats
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" | "jsonc" => "json",
        // Web
        "html" | "htm" => "html",
        "css" | "scss" | "less" => "css",
        // Markdown
        "md" | "markdown" => "markdown",
        // SQL
        "sql" => "sql",
        // Docker
        "dockerfile" => "dockerfile",
        // HCL/Terraform
        "tf" | "hcl" => "hcl",
        // Ruby
        "rb" => "ruby",
        // Lua
        "lua" => "lua",
        // Zig
        "zig" => "zig",
        // Nix
        "nix" => "nix",
        _ => return None,
    };

    Some(lang.to_string())
}

/// Get the single-line comment prefix for a language.
pub fn comment_prefix(language: &str) -> String {
    match language {
        "rust" | "c" | "cpp" | "java" | "javascript" | "typescript" | "go" | "zig" | "css" => {
            "//".to_string()
        }
        "python" | "bash" | "ruby" | "perl" | "toml" | "yaml" | "nix" => "#".to_string(),
        "lua" | "hcl" | "sql" => "--".to_string(),
        "html" => "<!--".to_string(),
        _ => "//".to_string(),
    }
}

/// Get human-readable display name for a language.
pub fn display_name(language: &str) -> &str {
    match language {
        "rust" => "Rust",
        "python" => "Python",
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "go" => "Go",
        "c" => "C",
        "cpp" => "C++",
        "java" => "Java",
        "bash" => "Shell",
        "toml" => "TOML",
        "yaml" => "YAML",
        "json" => "JSON",
        "html" => "HTML",
        "css" => "CSS",
        "markdown" => "Markdown",
        "sql" => "SQL",
        "text" => "Plain Text",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_by_extension() {
        assert_eq!(
            detect_from_extension(Path::new("main.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            detect_from_extension(Path::new("app.py")),
            Some("python".to_string())
        );
        assert_eq!(
            detect_from_extension(Path::new("index.tsx")),
            Some("typescript".to_string())
        );
        assert_eq!(
            detect_from_extension(Path::new("config.toml")),
            Some("toml".to_string())
        );
        assert_eq!(detect_from_extension(Path::new("unknown.xyz")), None);
    }

    #[test]
    fn test_detect_by_shebang() {
        let mut buf = Buffer::new();
        buf.insert(0, "#!/usr/bin/env python3\nprint('hello')");
        assert_eq!(detect_from_shebang(&buf), Some("python".to_string()));

        let mut buf2 = Buffer::new();
        buf2.insert(0, "#!/bin/bash\necho hello");
        assert_eq!(detect_from_shebang(&buf2), Some("bash".to_string()));
    }

    #[test]
    fn test_comment_prefix() {
        assert_eq!(comment_prefix("rust"), "//");
        assert_eq!(comment_prefix("python"), "#");
        assert_eq!(comment_prefix("lua"), "--");
    }
}
