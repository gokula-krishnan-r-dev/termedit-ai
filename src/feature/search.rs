/// Find and replace with regex support.

use regex::Regex;
use ropey::Rope;

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

/// Search engine for find and replace operations.
pub struct Search {
    /// Current search matches.
    pub matches: Vec<SearchMatch>,
    /// Current match index (for next/previous navigation).
    pub current_match: Option<usize>,
    /// The active search configuration.
    pub config: Option<SearchConfig>,
}

impl Search {
    /// Create a new empty search state.
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
            current_match: None,
            config: None,
        }
    }

    /// Execute a search on the given rope.
    pub fn find(&mut self, config: SearchConfig, rope: &Rope) -> &[SearchMatch] {
        self.matches.clear();
        self.current_match = None;

        if config.pattern.is_empty() {
            self.config = Some(config);
            return &self.matches;
        }

        if config.is_regex {
            let text = rope.to_string();
            self.find_regex(&config, &text);
        } else {
            self.find_literal_rope(&config, rope);
        }

        if !self.matches.is_empty() {
            self.current_match = Some(0);
        }

        self.config = Some(config);
        &self.matches
    }

    /// Find literal matches by scanning the rope (no full-document `String` copy).
    fn find_literal_rope(&mut self, config: &SearchConfig, rope: &Rope) {
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
            if config.whole_word && !self.is_whole_word_rope(rope, i, plen) {
                continue;
            }
            let matched: String = (0..plen).map(|k| rope.char(i + k)).collect();
            self.matches.push(SearchMatch {
                start: i,
                end: i + plen,
                text: matched,
            });
        }
    }

    fn is_whole_word_rope(&self, rope: &Rope, char_start: usize, pat_len_chars: usize) -> bool {
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

    /// Find literal text matches (used by tests and byte-oriented callers).
    fn find_literal(&mut self, config: &SearchConfig, text: &str) {
        let pattern = &config.pattern;
        let search_text;
        let search_pattern;

        if config.case_sensitive {
            search_text = text.to_string();
            search_pattern = pattern.to_string();
        } else {
            search_text = text.to_lowercase();
            search_pattern = pattern.to_lowercase();
        }

        let mut start = 0;
        while let Some(pos) = search_text[start..].find(&search_pattern) {
            let abs_pos = start + pos;
            // Convert byte offset to char offset
            let char_start = text[..abs_pos].chars().count();
            let char_end = char_start + pattern.chars().count();
            let matched_text = text[abs_pos..abs_pos + pattern.len()].to_string();

            if !config.whole_word || self.is_whole_word(text, abs_pos, pattern.len()) {
                self.matches.push(SearchMatch {
                    start: char_start,
                    end: char_end,
                    text: matched_text,
                });
            }

            start = abs_pos + pattern.len().max(1);
        }
    }

    /// Find regex matches.
    fn find_regex(&mut self, config: &SearchConfig, text: &str) {
        let regex_pattern = if config.case_sensitive {
            config.pattern.clone()
        } else {
            format!("(?i){}", config.pattern)
        };

        let Ok(re) = Regex::new(&regex_pattern) else {
            return; // Invalid regex, return empty
        };

        for mat in re.find_iter(text) {
            let char_start = text[..mat.start()].chars().count();
            let char_end = char_start + mat.as_str().chars().count();

            self.matches.push(SearchMatch {
                start: char_start,
                end: char_end,
                text: mat.as_str().to_string(),
            });
        }
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
}
