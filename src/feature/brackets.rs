/// Bracket matching for `()`, `[]`, and `{}`: highlights pairs and jump-to-bracket.
///
/// Uses a small lexer (line/block comments, double-quoted strings, `` ` `` literals)
/// so brackets inside those regions are ignored. Not language-perfect (e.g. Rust
/// lifetimes, raw strings) but avoids most false positives.

use crate::core::buffer::Buffer;

const PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];

fn pair_for(c: char) -> Option<(char, char)> {
    for &(o, cl) in PAIRS {
        if c == o || c == cl {
            return Some((o, cl));
        }
    }
    None
}

fn is_opening(c: char) -> bool {
    PAIRS.iter().any(|&(o, _)| o == c)
}

fn is_closing(c: char) -> bool {
    PAIRS.iter().any(|&(_, cl)| cl == c)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lex {
    Code,
    LineComment,
    BlockComment,
    StringDbl {
        escape: bool,
    },
    BacktickString,
}

struct Scanner<'a> {
    buf: &'a Buffer,
    i: usize,
    lex: Lex,
}

impl<'a> Scanner<'a> {
    fn new(buf: &'a Buffer) -> Self {
        Self {
            buf,
            i: 0,
            lex: Lex::Code,
        }
    }

    fn len(&self) -> usize {
        self.buf.len_chars()
    }

    fn peek(&self) -> Option<char> {
        if self.i < self.len() {
            Some(self.buf.rope.char(self.i))
        } else {
            None
        }
    }

    fn peek2(&self) -> Option<char> {
        if self.i + 1 < self.len() {
            Some(self.buf.rope.char(self.i + 1))
        } else {
            None
        }
    }

    /// Advance one codepoint; returns false if at EOF.
    fn bump_code(&mut self) -> bool {
        if self.i >= self.len() {
            return false;
        }
        self.i += 1;
        true
    }

    /// Process the current character according to `lex` (may consume 2 chars for
    /// `//`, `/*`, `*/`).
    fn step(&mut self) -> bool {
        let Some(c) = self.peek() else {
            return false;
        };
        match self.lex {
            Lex::Code => {
                if c == '/' && self.peek2() == Some('/') {
                    self.i += 2;
                    self.lex = Lex::LineComment;
                    return true;
                }
                if c == '/' && self.peek2() == Some('*') {
                    self.i += 2;
                    self.lex = Lex::BlockComment;
                    return true;
                }
                if c == '"' {
                    self.i += 1;
                    self.lex = Lex::StringDbl { escape: false };
                    return true;
                }
                if c == '`' {
                    self.i += 1;
                    self.lex = Lex::BacktickString;
                    return true;
                }
                self.i += 1;
                true
            }
            Lex::LineComment => {
                self.i += 1;
                if c == '\n' {
                    self.lex = Lex::Code;
                }
                true
            }
            Lex::BlockComment => {
                if c == '*' && self.peek2() == Some('/') {
                    self.i += 2;
                    self.lex = Lex::Code;
                } else {
                    self.i += 1;
                }
                true
            }
            Lex::StringDbl { escape } => {
                if escape {
                    self.i += 1;
                    self.lex = Lex::StringDbl { escape: false };
                    return true;
                }
                if c == '\\' {
                    self.i += 1;
                    self.lex = Lex::StringDbl { escape: true };
                    return true;
                }
                if c == '"' {
                    self.i += 1;
                    self.lex = Lex::Code;
                    return true;
                }
                self.i += 1;
                true
            }
            Lex::BacktickString => {
                if c == '`' {
                    self.i += 1;
                    self.lex = Lex::Code;
                } else {
                    self.i += 1;
                }
                true
            }
        }
    }

    /// Run until `i >= target` (exclusive): scanner is positioned to process `target`.
    fn run_to(&mut self, target: usize) {
        let target = target.min(self.len());
        while self.i < target {
            if !self.step() {
                break;
            }
        }
    }

    /// True if the next character to process is in "code" context (including at EOF).
    fn next_is_code(&self) -> bool {
        matches!(self.lex, Lex::Code)
    }
}

/// Resolve which character index holds a bracket relevant to the cursor (VS Code–style:
/// prefer the character at the cursor; if none, the one immediately before).
pub fn resolve_bracket_index(buffer: &Buffer, line: usize, col: usize) -> Option<usize> {
    let line_start = buffer.line_to_char(line);
    let col = col.min(buffer.line_len(line));
    let idx = line_start + col;
    let len = buffer.len_chars();
    if len == 0 {
        return None;
    }
    if idx < len {
        let c = buffer.rope.char(idx);
        if pair_for(c).is_some() {
            return Some(idx);
        }
    }
    if idx > 0 {
        let c = buffer.rope.char(idx - 1);
        if pair_for(c).is_some() {
            return Some(idx - 1);
        }
    }
    None
}

/// Returns the two absolute character indices of the matching pair (sorted low, high).
pub fn matching_pair(buffer: &Buffer, bracket_idx: usize) -> Option<(usize, usize)> {
    let len = buffer.len_chars();
    if bracket_idx >= len {
        return None;
    }
    let c = buffer.rope.char(bracket_idx);
    let (open_ch, close_ch) = pair_for(c)?;
    if is_opening(c) {
        find_forward(buffer, bracket_idx, open_ch, close_ch)
    } else if is_closing(c) {
        find_backward(buffer, bracket_idx, open_ch, close_ch)
    } else {
        None
    }
}

fn find_forward(
    buffer: &Buffer,
    open_idx: usize,
    open_ch: char,
    close_ch: char,
) -> Option<(usize, usize)> {
    let mut scan = Scanner::new(buffer);
    scan.run_to(open_idx);
    if !scan.next_is_code() {
        return None;
    }
    if scan.peek() != Some(open_ch) {
        return None;
    }
    let _ = scan.bump_code();
    let mut depth: i32 = 1;

    while scan.i < scan.len() {
        let was_code = scan.next_is_code();
        let pos = scan.i;
        let cur = scan.peek()?;
        if was_code {
            if cur == open_ch {
                depth += 1;
            } else if cur == close_ch {
                depth -= 1;
                if depth == 0 {
                    return Some((open_idx, pos));
                }
            }
        }
        if !scan.step() {
            break;
        }
    }
    None
}

fn find_backward(
    buffer: &Buffer,
    close_idx: usize,
    open_ch: char,
    close_ch: char,
) -> Option<(usize, usize)> {
    let mut scan = Scanner::new(buffer);
    let mut stack: Vec<usize> = vec![];

    while scan.i < buffer.len_chars() {
        let was_code = scan.next_is_code();
        let pos = scan.i;
        let c = scan.peek()?;

        if was_code {
            if c == open_ch {
                stack.push(pos);
            } else if c == close_ch {
                let open = stack.pop()?;
                if pos == close_idx {
                    return Some((open, close_idx));
                }
            }
        }

        if !scan.step() {
            break;
        }
    }
    None
}

/// Cursor (line/col) → matching bracket pair, or `None` if no bracket / no match / out of bounds.
pub fn matching_bracket_pair_at_cursor(
    buffer: &Buffer,
    line: usize,
    col: usize,
    max_chars: usize,
) -> Option<(usize, usize)> {
    if buffer.len_chars() > max_chars {
        return None;
    }
    let bidx = resolve_bracket_index(buffer, line, col)?;
    matching_pair(buffer, bidx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::buffer::Buffer;

    fn buf(s: &str) -> Buffer {
        let mut b = Buffer::new();
        b.insert(0, s);
        b.modified = false;
        b
    }

    #[test]
    fn nested_parens_inner_and_outer() {
        let s = "a ( ( x ) ) b";
        let b = buf(s);
        assert_eq!(matching_pair(&b, 4), Some((4, 8)));
        assert_eq!(matching_pair(&b, 8), Some((4, 8)));
        assert_eq!(matching_pair(&b, 2), Some((2, 10)));
    }

    #[test]
    fn nested_braces_and_mix() {
        let s = "fn main() { let x = (1 + (2)); }";
        let b = buf(s);
        let open_outer = s.find('{').unwrap();
        let close_outer = s.rfind('}').unwrap();
        assert_eq!(matching_pair(&b, open_outer), Some((open_outer, close_outer)));
    }

    #[test]
    fn ignores_brackets_in_string() {
        // The `)` inside `"…"` must not close `call(`.
        let s = r#"call(")") + ("(" )"#;
        let b = buf(s);
        let open = s.find('(').unwrap();
        assert_eq!(matching_pair(&b, open), Some((open, 8)));
    }

    #[test]
    fn ignores_line_comment() {
        let s = "( // )\nx)";
        let b = buf(s);
        let open = s.find('(').unwrap();
        let close = s.rfind(')').unwrap();
        assert_eq!(matching_pair(&b, open), Some((open, close)));
    }

    #[test]
    fn resolve_cursor_after_closer() {
        let s = "if (ok) { }";
        let b = buf(s);
        let close_paren = s.find(')').unwrap();
        // cursor column just after ')' — points past the bracket; resolve uses idx-1
        let line = 0usize;
        let col = close_paren + 1;
        let bi = resolve_bracket_index(&b, line, col).unwrap();
        assert_eq!(b.rope.char(bi), ')');
        assert_eq!(matching_bracket_pair_at_cursor(&b, line, col, 1_000_000), Some((s.find('(').unwrap(), close_paren)));
    }
}
