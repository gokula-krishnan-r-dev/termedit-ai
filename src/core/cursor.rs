/// Cursor positioning and selection management.
///
/// Supports three selection modes: character, line, and block (column).
/// Tab-aware column tracking with sticky column for vertical movement.

use crate::core::buffer::Buffer;

/// Selection mode for text selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// Normal character-by-character selection (Shift+Arrow).
    Char,
    /// Full line selection (triple-click or Ctrl+L).
    Line,
    /// Block/column selection (Alt+Shift+Arrow).
    Block,
}

/// Represents the anchor point of a selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Line of the selection anchor (where selection started).
    pub anchor_line: usize,
    /// Column of the selection anchor.
    pub anchor_col: usize,
    /// Selection mode.
    pub mode: SelectionMode,
}

/// The editor cursor with position tracking and selection state.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// Current line (0-indexed).
    pub line: usize,
    /// Current visual column (0-indexed, tab-aware).
    pub col: usize,
    /// Desired column when moving vertically (sticky column).
    /// Preserves horizontal position when moving through shorter lines.
    pub col_target: usize,
    /// Active selection, if any.
    pub selection: Option<Selection>,
}

impl Cursor {
    /// Create a new cursor at position (0, 0).
    pub fn new() -> Self {
        Self {
            line: 0,
            col: 0,
            col_target: 0,
            selection: None,
        }
    }

    /// Create a cursor at a specific position.
    pub fn at(line: usize, col: usize) -> Self {
        Self {
            line,
            col,
            col_target: col,
            selection: None,
        }
    }

    /// Move cursor left by one character.
    pub fn move_left(&mut self, buffer: &Buffer) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = buffer.line_len(self.line);
        }
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Move cursor right by one character.
    pub fn move_right(&mut self, buffer: &Buffer) {
        let line_len = buffer.line_len(self.line);
        if self.col < line_len {
            self.col += 1;
        } else if self.line < buffer.line_count().saturating_sub(1) {
            self.line += 1;
            self.col = 0;
        }
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Move cursor up by one line, preserving sticky column.
    pub fn move_up(&mut self, buffer: &Buffer) {
        if self.line > 0 {
            self.line -= 1;
            self.col = self.col_target.min(buffer.line_len(self.line));
        }
        self.clear_selection();
    }

    /// Move cursor down by one line, preserving sticky column.
    pub fn move_down(&mut self, buffer: &Buffer) {
        if self.line < buffer.line_count().saturating_sub(1) {
            self.line += 1;
            self.col = self.col_target.min(buffer.line_len(self.line));
        }
        self.clear_selection();
    }

    /// Move cursor to the beginning of the line.
    pub fn move_home(&mut self, _buffer: &Buffer) {
        self.col = 0;
        self.col_target = 0;
        self.clear_selection();
    }

    /// Move cursor to the end of the line.
    pub fn move_end(&mut self, buffer: &Buffer) {
        self.col = buffer.line_len(self.line);
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Move cursor to the start of the document.
    pub fn move_file_start(&mut self) {
        self.line = 0;
        self.col = 0;
        self.col_target = 0;
        self.clear_selection();
    }

    /// Move cursor to the end of the document.
    pub fn move_file_end(&mut self, buffer: &Buffer) {
        self.line = buffer.line_count().saturating_sub(1);
        self.col = buffer.line_len(self.line);
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Jump one word to the left (Ctrl+Left).
    pub fn word_left(&mut self, buffer: &Buffer) {
        if self.col == 0 {
            if self.line > 0 {
                self.line -= 1;
                self.col = buffer.line_len(self.line);
            }
        } else {
            let text = buffer.line_text(self.line);
            let chars: Vec<char> = text.chars().collect();
            let mut pos = self.col.min(chars.len());

            // Skip whitespace backwards
            while pos > 0 && chars[pos - 1].is_whitespace() {
                pos -= 1;
            }
            // Skip word characters backwards
            while pos > 0 && !chars[pos - 1].is_whitespace() && chars[pos - 1].is_alphanumeric()
            {
                pos -= 1;
            }
            self.col = pos;
        }
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Jump one word to the right (Ctrl+Right).
    pub fn word_right(&mut self, buffer: &Buffer) {
        let line_len = buffer.line_len(self.line);
        if self.col >= line_len {
            if self.line < buffer.line_count().saturating_sub(1) {
                self.line += 1;
                self.col = 0;
            }
        } else {
            let text = buffer.line_text(self.line);
            let chars: Vec<char> = text.chars().collect();
            let mut pos = self.col;

            // Skip word characters forward
            while pos < chars.len() && chars[pos].is_alphanumeric() {
                pos += 1;
            }
            // Skip whitespace/punctuation forward
            while pos < chars.len() && !chars[pos].is_alphanumeric() {
                pos += 1;
            }
            self.col = pos;
        }
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Start or extend a selection from current position.
    pub fn start_selection(&mut self, mode: SelectionMode) {
        if self.selection.is_none() {
            self.selection = Some(Selection {
                anchor_line: self.line,
                anchor_col: self.col,
                mode,
            });
        }
    }

    /// Move left while extending selection.
    pub fn select_left(&mut self, buffer: &Buffer) {
        self.start_selection(SelectionMode::Char);
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = buffer.line_len(self.line);
        }
        self.col_target = self.col;
    }

    /// Move right while extending selection.
    pub fn select_right(&mut self, buffer: &Buffer) {
        self.start_selection(SelectionMode::Char);
        let line_len = buffer.line_len(self.line);
        if self.col < line_len {
            self.col += 1;
        } else if self.line < buffer.line_count().saturating_sub(1) {
            self.line += 1;
            self.col = 0;
        }
        self.col_target = self.col;
    }

    /// Move up while extending selection.
    pub fn select_up(&mut self, buffer: &Buffer) {
        self.start_selection(SelectionMode::Char);
        if self.line > 0 {
            self.line -= 1;
            self.col = self.col_target.min(buffer.line_len(self.line));
        }
    }

    /// Move down while extending selection.
    pub fn select_down(&mut self, buffer: &Buffer) {
        self.start_selection(SelectionMode::Char);
        if self.line < buffer.line_count().saturating_sub(1) {
            self.line += 1;
            self.col = self.col_target.min(buffer.line_len(self.line));
        }
    }

    /// Select all text in the buffer.
    pub fn select_all(&mut self, buffer: &Buffer) {
        self.selection = Some(Selection {
            anchor_line: 0,
            anchor_col: 0,
            mode: SelectionMode::Char,
        });
        self.line = buffer.line_count().saturating_sub(1);
        self.col = buffer.line_len(self.line);
        self.col_target = self.col;
    }

    /// Select the current line.
    pub fn select_line(&mut self, buffer: &Buffer) {
        self.selection = Some(Selection {
            anchor_line: self.line,
            anchor_col: 0,
            mode: SelectionMode::Line,
        });
        if self.line < buffer.line_count().saturating_sub(1) {
            self.line += 1;
            self.col = 0;
        } else {
            self.col = buffer.line_len(self.line);
        }
        self.col_target = self.col;
    }

    /// Clear current selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Check if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    /// Get the selected character range as (start_char_idx, end_char_idx).
    ///
    /// Returns None if no selection is active.
    pub fn selection_range(&self, buffer: &Buffer) -> Option<(usize, usize)> {
        let sel = self.selection.as_ref()?;

        let anchor_char = buffer.line_to_char(sel.anchor_line) + sel.anchor_col;
        let cursor_char = buffer.line_to_char(self.line) + self.col;

        let start = anchor_char.min(cursor_char);
        let end = anchor_char.max(cursor_char);

        Some((start, end))
    }

    /// Get selected text from the buffer.
    pub fn selected_text(&self, buffer: &Buffer) -> Option<String> {
        let (start, end) = self.selection_range(buffer)?;
        let slice = buffer.rope.slice(start..end);
        Some(slice.to_string())
    }

    /// Convert the cursor position to a character index in the buffer.
    pub fn char_index(&self, buffer: &Buffer) -> usize {
        let line_start = buffer.line_to_char(self.line);
        let col = self.col.min(buffer.line_len(self.line));
        line_start + col
    }

    /// Clamp cursor position to valid buffer bounds.
    pub fn clamp(&mut self, buffer: &Buffer) {
        let max_line = buffer.line_count().saturating_sub(1);
        self.line = self.line.min(max_line);
        self.col = self.col.min(buffer.line_len(self.line));
    }

    /// Move cursor to a specific line and column.
    pub fn goto(&mut self, line: usize, col: usize, buffer: &Buffer) {
        let max_line = buffer.line_count().saturating_sub(1);
        self.line = line.min(max_line);
        self.col = col.min(buffer.line_len(self.line));
        self.col_target = self.col;
        self.clear_selection();
    }

    /// Move cursor up by a page (for PageUp).
    pub fn page_up(&mut self, page_size: usize, buffer: &Buffer) {
        self.line = self.line.saturating_sub(page_size);
        self.col = self.col_target.min(buffer.line_len(self.line));
        self.clear_selection();
    }

    /// Move cursor down by a page (for PageDown).
    pub fn page_down(&mut self, page_size: usize, buffer: &Buffer) {
        let max_line = buffer.line_count().saturating_sub(1);
        self.line = (self.line + page_size).min(max_line);
        self.col = self.col_target.min(buffer.line_len(self.line));
        self.clear_selection();
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer(text: &str) -> Buffer {
        let mut buf = Buffer::new();
        buf.insert(0, text);
        buf.modified = false;
        buf
    }

    #[test]
    fn test_new_cursor() {
        let c = Cursor::new();
        assert_eq!(c.line, 0);
        assert_eq!(c.col, 0);
        assert!(!c.has_selection());
    }

    #[test]
    fn test_move_right_left() {
        let buf = make_buffer("Hello");
        let mut c = Cursor::new();

        c.move_right(&buf);
        assert_eq!(c.col, 1);

        c.move_right(&buf);
        c.move_right(&buf);
        assert_eq!(c.col, 3);

        c.move_left(&buf);
        assert_eq!(c.col, 2);
    }

    #[test]
    fn test_move_up_down() {
        let buf = make_buffer("Line 1\nLine 2\nLine 3");
        let mut c = Cursor::new();

        c.move_down(&buf);
        assert_eq!(c.line, 1);

        c.move_down(&buf);
        assert_eq!(c.line, 2);

        // Should not go past last line
        c.move_down(&buf);
        assert_eq!(c.line, 2);

        c.move_up(&buf);
        assert_eq!(c.line, 1);
    }

    #[test]
    fn test_sticky_column() {
        let buf = make_buffer("Long line here\nHi\nAnother long line");
        let mut c = Cursor::new();

        // Move to end of first line
        c.move_end(&buf);
        assert_eq!(c.col, 14);
        assert_eq!(c.col_target, 14);

        // Move down to short line — col clamped
        c.move_down(&buf);
        assert_eq!(c.line, 1);
        assert_eq!(c.col, 2); // "Hi" is only 2 chars
        assert_eq!(c.col_target, 14); // target preserved

        // Move down to long line — col restored
        c.move_down(&buf);
        assert_eq!(c.line, 2);
        assert_eq!(c.col, 14); // restored from col_target
    }

    #[test]
    fn test_word_jump() {
        let buf = make_buffer("hello world foo");
        let mut c = Cursor::new();

        c.word_right(&buf);
        assert_eq!(c.col, 6); // after "hello "

        c.word_right(&buf);
        assert_eq!(c.col, 12); // after "world "

        c.word_left(&buf);
        assert_eq!(c.col, 6);
    }

    #[test]
    fn test_home_end() {
        let buf = make_buffer("Hello World");
        let mut c = Cursor::at(0, 5);

        c.move_home(&buf);
        assert_eq!(c.col, 0);

        c.move_end(&buf);
        assert_eq!(c.col, 11);
    }

    #[test]
    fn test_selection() {
        let buf = make_buffer("Hello World");
        let mut c = Cursor::new();

        c.select_right(&buf);
        c.select_right(&buf);
        c.select_right(&buf);

        assert!(c.has_selection());
        assert_eq!(c.selected_text(&buf), Some("Hel".to_string()));
    }

    #[test]
    fn test_select_all() {
        let buf = make_buffer("Hello\nWorld");
        let mut c = Cursor::new();

        c.select_all(&buf);
        assert_eq!(c.selected_text(&buf), Some("Hello\nWorld".to_string()));
    }

    #[test]
    fn test_line_wrap_movement() {
        let buf = make_buffer("AB\nCD");
        let mut c = Cursor::new();

        // Move right past line end -> wraps to next line
        c.move_right(&buf);
        c.move_right(&buf);
        c.move_right(&buf); // wraps to line 1, col 0
        assert_eq!(c.line, 1);
        assert_eq!(c.col, 0);

        // Move left past line start -> wraps to previous line
        c.move_left(&buf); // wraps to line 0, col 2
        assert_eq!(c.line, 0);
        assert_eq!(c.col, 2);
    }
}
