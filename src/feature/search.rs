/// Find and replace with regex support.

use std::collections::HashSet;

use regex::Regex;
use ropey::Rope;

use crate::core::document::Document;

/// A single search match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Start character index in the rope.
    pub start: usize,
    /// End character index in the rope (exclusive).
    pub end: usize,
    /// The matched text.
    pub text: String,
}

/// Search configuration.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// The search pattern.
    pub pattern: String,
    /// Whether to use regex matching.
    pub is_regex: bool,
    /// Whether the search is case-sensitive.
    pub case_sensitive: bool,
    /// Whether to match whole words only.
    pub whole_word: bool,
}

impl SearchConfig {
    /// Create a new literal search.
    pub fn literal(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex: false,
            case_sensitive: true,
            whole_word: false,
        }
    }

    /// Create a new case-insensitive search.
    pub fn case_insensitive(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex: false,
            case_sensitive: false,
            whole_word: false,
        }
    }
}

/// One match when searching across open tabs (for the picker UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTabHit {
    /// Index into the app's `documents` vector.
    pub tab_index: usize,
    /// [`Buffer::display_name`](crate::core::buffer::Buffer::display_name) for the tab.
    pub tab_label: String,
    /// Start character index in that buffer's rope.
    pub match_start: usize,
    /// Line index (0-based) containing the match.
    pub line: usize,
    /// Single-line preview (trimmed, may end with …).
    pub preview: String,
}

/// Collect matches without mutating [`Search`] state (literal uses rope scan; regex allocates `rope.to_string()`).
pub fn collect_matches(config: &SearchConfig, rope: &Rope) -> (Vec<SearchMatch>, Option<String>) {
    let mut matches = Vec::new();
    if config.pattern.is_empty() {
        return (matches, None);
    }
    if config.is_regex {
        let text = rope.to_string();
        let err = collect_regex_matches(config, rope, &text, &mut matches);
        (matches, err)
    } else {
        collect_literal_rope_matches(config, rope, &mut matches);
        (matches, None)
    }
}

fn is_whole_word_rope(rope: &Rope, char_start: usize, pat_len_chars: usize) -> bool {
    let before_ok = char_start == 0 || {
        let c = rope.char(char_start - 1);
        !c.is_alphanumeric() && c != '_'
    };
    let end = char_start + pat_len_chars;
    let after_ok = end >= rope.len_chars() || {
        let c = rope.char(end);
        !c.is_alphanumeric() && c != '_'
    };
    before_ok && after_ok
}

fn collect_literal_rope_matches(config: &SearchConfig, rope: &Rope, out: &mut Vec<SearchMatch>) {
    let pattern = &config.pattern;
    let pat_chars: Vec<char> = pattern.chars().collect();
    let plen = pat_chars.len();
    if plen == 0 {
        return;
    }
    let n = rope.len_chars();
    if plen > n {
        return;
    }

    let pat_lower: Option<Vec<String>> = if config.case_sensitive {
        None
    } else {
        Some(
            pat_chars
                .iter()
                .map(|c| c.to_lowercase().to_string())
                .collect(),
        )
    };

    'outer: for i in 0..=n - plen {
        for j in 0..plen {
            let rc = rope.char(i + j);
            let ok = if config.case_sensitive {
                rc == pat_chars[j]
            } else {
                rc.to_lowercase().to_string() == pat_lower.as_ref().unwrap()[j]
            };
            if !ok {
                continue 'outer;
            }
        }
        if config.whole_word && !is_whole_word_rope(rope, i, plen) {
            continue;
        }
        let matched: String = (0..plen).map(|k| rope.char(i + k)).collect();
        out.push(SearchMatch {
            start: i,
            end: i + plen,
            text: matched,
        });
    }
}

fn collect_regex_matches(
    config: &SearchConfig,
    rope: &Rope,
    text: &str,
    out: &mut Vec<SearchMatch>,
) -> Option<String> {
    let regex_pattern = if config.case_sensitive {
        config.pattern.clone()
    } else {
        format!("(?i){}", config.pattern)
    };

    let re = match Regex::new(&regex_pattern) {
        Ok(r) => r,
        Err(e) => return Some(format!("Invalid regex: {}", e)),
    };

    for mat in re.find_iter(text) {
        let char_start = text[..mat.start()].chars().count();
        let matched = mat.as_str();
        let char_len = matched.chars().count();
        if config.whole_word && !is_whole_word_rope(rope, char_start, char_len) {
            continue;
        }

        out.push(SearchMatch {
            start: char_start,
            end: char_start + char_len,
            text: matched.to_string(),
        });
    }
    None
}

/// Search all open tabs in order; skips buffers larger than `max_chars_per_tab` (counts as one skip each).
pub fn search_open_tabs(
    documents: &[Document],
    config: &SearchConfig,
    max_results: usize,
    max_chars_per_tab: usize,
) -> (Vec<OpenTabHit>, usize, Option<String>) {
    let mut hits = Vec::new();
    let mut skipped_tabs = 0usize;

    if config.pattern.is_empty() {
        return (hits, skipped_tabs, None);
    }

    if config.is_regex {
        let regex_pattern = if config.case_sensitive {
            config.pattern.clone()
        } else {
            format!("(?i){}", config.pattern)
        };
        if let Err(e) = Regex::new(&regex_pattern) {
            return (hits, skipped_tabs, Some(format!("Invalid regex: {}", e)));
        }
    }

    const PREVIEW_MAX: usize = 120;

    for (tab_index, doc) in documents.iter().enumerate() {
        if hits.len() >= max_results {
            break;
        }
        let rope = &doc.buffer.rope;
        let n = rope.len_chars();
        if n > max_chars_per_tab {
            skipped_tabs += 1;
            continue;
        }

        let (matches, err) = collect_matches(config, rope);
        if let Some(e) = err {
            return (hits, skipped_tabs, Some(e));
        }

        for m in matches {
            if hits.len() >= max_results {
                break;
            }
            let line = rope.char_to_line(m.start);
            let mut preview = doc.buffer.line_text(line);
            preview = preview.trim_end_matches('\n').trim_end_matches('\r').to_string();
            if preview.chars().count() > PREVIEW_MAX {
                let truncated: String = preview.chars().take(PREVIEW_MAX.saturating_sub(1)).collect();
                preview = format!("{}…", truncated);
            }
            hits.push(OpenTabHit {
                tab_index,
                tab_label: doc.buffer.display_name(),
                match_start: m.start,
                line,
                preview,
            });

            if hits.len() >= max_results {
                break;
            }
        }
    }

    (hits, skipped_tabs, None)
}

/// Search engine for find and replace operations.
pub struct Search {
    /// Current search matches.
    pub matches: Vec<SearchMatch>,
    /// Current match index (for next/previous navigation).
    pub current_match: Option<usize>,
    /// The active search configuration.
    pub config: Option<SearchConfig>,
    /// Line indices (0-based) that contain at least one match (for minimap strip).
    pub match_lines: HashSet<usize>,
    /// Last regex compile error, if any.
    pub last_error: Option<String>,
}

impl Search {
    /// Create a new empty search state.
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
            current_match: None,
            config: None,
            match_lines: HashSet::new(),
            last_error: None,
        }
    }

    /// Execute a search on the given rope.
    pub fn find(&mut self, config: SearchConfig, rope: &Rope) -> &[SearchMatch] {
        self.matches.clear();
        self.current_match = None;
        self.match_lines.clear();
        self.last_error = None;

        if config.pattern.is_empty() {
            self.config = Some(config);
            return &self.matches;
        }

        let (matches, err) = collect_matches(&config, rope);
        self.matches = matches;
        self.last_error = err;

        if !self.matches.is_empty() {
            self.current_match = Some(0);
            for m in &self.matches {
                let line = rope.char_to_line(m.start);
                self.match_lines.insert(line);
            }
        }

        self.config = Some(config);
        &self.matches
    }

    /// Check if the match at byte position is a whole word.
    fn is_whole_word(&self, text: &str, byte_pos: usize, byte_len: usize) -> bool {
        let before_ok = byte_pos == 0
            || text[..byte_pos]
                .chars()
                .last()
                .map_or(true, |c| !c.is_alphanumeric() && c != '_');
        let after_ok = byte_pos + byte_len >= text.len()
            || text[byte_pos + byte_len..]
                .chars()
                .next()
                .map_or(true, |c| !c.is_alphanumeric() && c != '_');
        before_ok && after_ok
    }

    /// Navigate to the next match. Returns the match index.
    pub fn next_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        let idx = match self.current_match {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        };
        self.current_match = Some(idx);
        Some(&self.matches[idx])
    }

    /// Navigate to the previous match. Returns the match index.
    pub fn prev_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        let idx = match self.current_match {
            Some(0) | None => self.matches.len() - 1,
            Some(i) => i - 1,
        };
        self.current_match = Some(idx);
        Some(&self.matches[idx])
    }

    /// Get the current match.
    pub fn current(&self) -> Option<&SearchMatch> {
        self.current_match
            .and_then(|i| self.matches.get(i))
    }

    /// Line index (0-based) of the current match, if any.
    pub fn current_match_line(&self, rope: &Rope) -> Option<usize> {
        self.current().map(|m| rope.char_to_line(m.start))
    }

    /// Find the nearest match to a character position.
    pub fn find_nearest(&mut self, char_pos: usize) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }

        let idx = self
            .matches
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| {
                if m.start >= char_pos {
                    m.start - char_pos
                } else {
                    char_pos - m.start
                }
            })
            .map(|(i, _)| i)?;

        self.current_match = Some(idx);
        Some(&self.matches[idx])
    }

    /// Get total match count.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Get the display string "X of Y" for status bar.
    pub fn status_text(&self) -> String {
        match self.current_match {
            Some(i) => format!("{} of {}", i + 1, self.matches.len()),
            None if self.matches.is_empty() => "No results".to_string(),
            None => format!("{} found", self.matches.len()),
        }
    }

    /// Clear all search state.
    pub fn clear(&mut self) {
        self.matches.clear();
        self.current_match = None;
        self.config = None;
        self.match_lines.clear();
        self.last_error = None;
    }

    /// Status line for the find bar (includes regex errors).
    pub fn find_bar_status(&self) -> String {
        if let Some(ref e) = self.last_error {
            let short: String = e.chars().take(40).collect();
            return short;
        }
        self.status_text()
    }

    /// Check if a character position falls within any match.
    pub fn is_match_at(&self, char_pos: usize) -> bool {
        self.matches
            .iter()
            .any(|m| char_pos >= m.start && char_pos < m.end)
    }
}

impl Default for Search {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_literal() {
        let rope = Rope::from_str("hello world hello");
        let mut search = Search::new();
        search.find(SearchConfig::literal("hello"), &rope);

        assert_eq!(search.match_count(), 2);
        assert_eq!(search.matches[0].start, 0);
        assert_eq!(search.matches[0].end, 5);
        assert_eq!(search.matches[1].start, 12);
    }

    #[test]
    fn test_find_case_insensitive() {
        let rope = Rope::from_str("Hello HELLO hello");
        let mut search = Search::new();
        search.find(SearchConfig::case_insensitive("hello"), &rope);

        assert_eq!(search.match_count(), 3);
    }

    #[test]
    fn test_find_regex() {
        let rope = Rope::from_str("foo123 bar456");
        let mut search = Search::new();
        search.find(
            SearchConfig {
                pattern: r"\d+".to_string(),
                is_regex: true,
                case_sensitive: true,
                whole_word: false,
            },
            &rope,
        );

        assert_eq!(search.match_count(), 2);
        assert_eq!(search.matches[0].text, "123");
        assert_eq!(search.matches[1].text, "456");
    }

    #[test]
    fn test_next_prev_match() {
        let rope = Rope::from_str("aaa bbb aaa bbb aaa");
        let mut search = Search::new();
        search.find(SearchConfig::literal("aaa"), &rope);

        assert_eq!(search.match_count(), 3);

        let m = search.next_match().unwrap().clone();
        assert_eq!(m.start, 8); // wraps from idx 0 -> 1

        let m = search.prev_match().unwrap().clone();
        assert_eq!(m.start, 0);
    }

    #[test]
    fn test_empty_pattern() {
        let rope = Rope::from_str("hello");
        let mut search = Search::new();
        search.find(SearchConfig::literal(""), &rope);
        assert_eq!(search.match_count(), 0);
    }

    #[test]
    fn test_no_matches() {
        let rope = Rope::from_str("hello");
        let mut search = Search::new();
        search.find(SearchConfig::literal("xyz"), &rope);
        assert_eq!(search.match_count(), 0);
        assert_eq!(search.status_text(), "No results");
    }

    #[test]
    fn test_regex_whole_word() {
        let rope = Rope::from_str("foobaz foo foobar");
        let mut search = Search::new();
        search.find(
            SearchConfig {
                pattern: "foo".to_string(),
                is_regex: true,
                case_sensitive: true,
                whole_word: true,
            },
            &rope,
        );
        assert_eq!(search.match_count(), 1);
        assert_eq!(search.matches[0].text, "foo");
    }

    #[test]
    fn test_invalid_regex_sets_error() {
        let rope = Rope::from_str("hello");
        let mut search = Search::new();
        search.find(
            SearchConfig {
                pattern: r"(".to_string(),
                is_regex: true,
                case_sensitive: true,
                whole_word: false,
            },
            &rope,
        );
        assert_eq!(search.match_count(), 0);
        assert!(search.last_error.is_some());
    }

    #[test]
    fn collect_matches_literal_empty_pattern() {
        let rope = Rope::from_str("abc");
        let (m, e) = collect_matches(&SearchConfig::literal(""), &rope);
        assert!(m.is_empty());
        assert!(e.is_none());
    }

    #[test]
    fn collect_matches_literal_matches_find() {
        let rope = Rope::from_str("hello world hello");
        let cfg = SearchConfig::literal("hello");
        let (m, e) = collect_matches(&cfg, &rope);
        assert!(e.is_none());
        let mut search = Search::new();
        search.find(cfg, &rope);
        assert_eq!(m, search.matches);
    }

    #[test]
    fn collect_matches_invalid_regex() {
        let rope = Rope::from_str("x");
        let cfg = SearchConfig {
            pattern: r"(".to_string(),
            is_regex: true,
            case_sensitive: true,
            whole_word: false,
        };
        let (m, e) = collect_matches(&cfg, &rope);
        assert!(m.is_empty());
        assert!(e.is_some());
    }

    #[test]
    fn search_open_tabs_two_buffers() {
        let mut a = Document::new();
        a.insert_text("fn foo() {}\n");
        let mut b = Document::new();
        b.insert_text("foo bar\nbaz foo\n");

        let docs = vec![a, b];
        let cfg = SearchConfig::case_insensitive("foo");
        let (hits, skipped, err) = search_open_tabs(&docs, &cfg, 100, 2_000_000);
        assert!(err.is_none());
        assert_eq!(skipped, 0);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].tab_index, 0);
        assert_eq!(hits[0].tab_label, "Untitled");
        assert_eq!(hits[1].tab_index, 1);
        assert_eq!(hits[2].tab_index, 1);
    }

    #[test]
    fn search_open_tabs_skips_large_buffer() {
        let mut huge = Document::new();
        huge.insert_text(&"x".repeat(100));
        let small = Document::new();
        let docs = vec![huge, small];
        let cfg = SearchConfig::literal("x");
        let (_hits, skipped, err) = search_open_tabs(&docs, &cfg, 100, 10);
        assert!(err.is_none());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn search_open_tabs_max_results() {
        let mut d = Document::new();
        d.insert_text("a a a a a ");
        let docs = vec![d];
        let cfg = SearchConfig::literal("a");
        let (hits, _, _) = search_open_tabs(&docs, &cfg, 2, 2_000_000);
        assert_eq!(hits.len(), 2);
    }
}
