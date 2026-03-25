//! Log summarization for token control.

/// Take the last `max_lines` lines; returns (text, line_count_in_sample).
pub fn summarize_log_bytes(text: &str, max_lines: usize) -> (String, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    if total <= max_lines {
        return (lines.join("\n"), total);
    }
    let start = total.saturating_sub(max_lines);
    let slice = &lines[start..];
    // Prefer lines that look like errors if we had room — keep tail-only for stability.
    (slice.join("\n"), max_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_only() {
        let s = (0..50).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n");
        let (out, n) = summarize_log_bytes(&s, 10);
        assert_eq!(n, 10);
        assert!(out.contains("line49"));
    }
}
