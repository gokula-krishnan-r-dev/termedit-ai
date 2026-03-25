/// Local completion provider for ghost text (inline suggestions).
///
/// Returns an optional suggestion suffix to display after the cursor.
/// Phase 1: pattern-based and buffer heuristics; extension point for AI later.

use crate::core::document::Document;

/// Compute a ghost text suggestion for the current cursor position.
/// Returns the suffix to show after the cursor (e.g. `"* FROM users;"`).
pub fn suggest(doc: &Document) -> Option<String> {
    let line = doc.buffer.line_text(doc.cursor.line);
    let col = doc.cursor.col;
    let prefix: String = line.chars().take(col).collect();
    let trimmed = prefix.trim_end();

    // Pattern-based suggestions (language-agnostic prefixes)
    // Instant suggestions without trailing space for SELECT / INSERT
    if trimmed.ends_with("SELECT ") {
        return Some("* FROM users;".to_string());
    }
    if trimmed.ends_with("SELECT") {
        return Some(" * FROM users;".to_string());
    }
    if trimmed.ends_with("INSERT INTO ") {
        return Some("users (name, email, password) VALUES ('John Doe', 'john.doe@example.com', 'password');".to_string());
    }
    if trimmed.ends_with("INSERT INTO") {
        return Some(" users (name, email, password) VALUES (...);".to_string());
    }
    if trimmed.ends_with("INSERT ") {
        return Some("INTO ".to_string());
    }
    if trimmed.ends_with("INSERT") {
        return Some(" INTO ".to_string());
    }
    if trimmed.ends_with("def ") {
        return Some("pass".to_string());
    }
    if trimmed.ends_with("if ") || trimmed.ends_with("elif ") {
        return Some(": pass".to_string());
    }
    if trimmed.ends_with("for ") || trimmed.ends_with("while ") {
        return Some(": pass".to_string());
    }
    if trimmed.ends_with("import ") {
        return Some("os".to_string());
    }
    if trimmed.ends_with("return ") {
        return Some("None".to_string());
    }
    if trimmed.ends_with("class ") {
        return Some(": pass".to_string());
    }
    if trimmed.ends_with("try:") || trimmed == "try:" {
        return Some("\n    pass\nexcept Exception:\n    pass".to_string());
    }

    // Optional: buffer word completion (suggest rest of current word from buffer)
    if !trimmed.is_empty() {
        let last_word_start = trimmed
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let word_prefix = &trimmed[last_word_start..];
        if word_prefix.len() >= 2 {
            if let Some(rest) = complete_word_from_buffer(doc, word_prefix) {
                return Some(rest);
            }
        }
    }

    None
}

/// Suggest completion for current word from other buffer content (simple prefix match).
fn complete_word_from_buffer(doc: &Document, word_prefix: &str) -> Option<String> {
    use std::collections::HashSet;
    let line_count = doc.buffer.line_count();
    let mut candidates: HashSet<String> = HashSet::new();
    for line_idx in 0..line_count {
        let text = doc.buffer.line_text(line_idx);
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if word.starts_with(word_prefix) && word != word_prefix && word.len() > word_prefix.len()
            {
                candidates.insert(word[word_prefix.len()..].to_string());
            }
        }
    }
    // Prefer shortest suffix (most likely completion)
    let mut best: Option<String> = None;
    for suffix in candidates {
        if best.as_ref().map_or(true, |b| suffix.len() < b.len()) {
            best = Some(suffix);
        }
    }
    best
}

/// Common keywords for completion dropdown (SQL, Python, etc.).
const KEYWORDS: &[&str] = &[
    "AUTO_INCREMENT", "CREATE", "DELETE", "FROM", "INDEX", "INSERT", "INT", "INTO",
    "KEY", "NOT", "NULL", "PRIMARY", "SELECT", "TABLE", "UPDATE", "VALUES",
    "VARCHAR", "WHERE", "AND", "OR", "AS", "ON", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from",
    "try", "except", "finally", "with", "pass", "break", "continue", "lambda", "None", "True", "False",
];

/// Return a list of completion items for the current word prefix and the prefix length.
/// Used for the inline completion dropdown (e.g. INDEX, INSERT, INT when typing "IN").
pub fn suggest_list(doc: &Document) -> Option<(Vec<String>, usize)> {
    let line = doc.buffer.line_text(doc.cursor.line);
    let col = doc.cursor.col;
    let prefix: String = line.chars().take(col).collect();
    let trimmed = prefix.trim_end();
    let word_start = trimmed
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let word_prefix = &trimmed[word_start..];
    if word_prefix.is_empty() {
        return None;
    }
    let prefix_len = word_prefix.chars().count();
    let mut items: std::collections::HashSet<String> = std::collections::HashSet::new();
    let lower = word_prefix.to_lowercase();
    for &kw in KEYWORDS {
        if kw.to_lowercase().starts_with(&lower) && kw != word_prefix {
            items.insert(kw.to_string());
        }
    }
    for line_idx in 0..doc.buffer.line_count() {
        let text = doc.buffer.line_text(line_idx);
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if word.len() > prefix_len && word.starts_with(word_prefix) {
                items.insert(word.to_string());
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    let mut list: Vec<String> = items.into_iter().collect();
    list.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    let max_items = 18;
    if list.len() > max_items {
        list.truncate(max_items);
    }
    Some((list, prefix_len))
}
