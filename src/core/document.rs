/// Document: wraps Buffer + Cursor + History into a single editable unit.
///
/// Each open file is represented by one Document. All editing operations
/// go through Document to keep the buffer, cursor, and history in sync.

use std::path::Path;

use crate::core::buffer::Buffer;
use crate::core::cursor::Cursor;
use crate::core::history::{EditCommand, History};
use crate::error::Result;
use crate::feature::search::SearchMatch;
#[cfg(feature = "timeline")]
use crate::feature::timeline::TimelineSender;
#[cfg(feature = "timeline")]
use crate::feature::timeline::models::{TimelineOp, TimelineEvent};

/// Represents a single open file/buffer with all editing state.
pub struct Document {
    /// The text buffer.
    pub buffer: Buffer,
    /// The cursor and selection state.
    pub cursor: Cursor,
    /// Undo/redo history.
    pub history: History,
    /// Detected language name (e.g., "rust", "python").
    pub language: String,
    /// Vertical scroll offset (first visible line).
    pub scroll_y: usize,
    /// Horizontal scroll offset.
    pub scroll_x: usize,
    #[cfg(feature = "timeline")]
    pub timeline_sender: Option<TimelineSender>,
}

impl Document {
    /// Create a new empty document.
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            cursor: Cursor::new(),
            history: History::new(),
            language: "text".to_string(),
            scroll_y: 0,
            scroll_x: 0,
            #[cfg(feature = "timeline")]
            timeline_sender: None,
        }
    }

    /// Open a file and create a document from it.
    ///
    /// If the path does not exist yet, opens an empty buffer with that path set (save will create the file).
    pub fn open(path: &Path) -> Result<Self> {
        let buffer = Buffer::from_file_or_new(path)?;
        let language = crate::feature::language::detect_language(path, &buffer);
        #[cfg(feature = "timeline")]
        let timeline_sender = if path.starts_with(dirs::data_local_dir().unwrap_or_default()) {
            None
        } else {
            let sender = crate::feature::timeline::start_worker(path.to_path_buf());
            sender.send_init(buffer.to_string());
            Some(sender)
        };

        Ok(Self {
            buffer,
            cursor: Cursor::new(),
            history: History::new(),
            language,
            scroll_y: 0,
            scroll_x: 0,
            #[cfg(feature = "timeline")]
            timeline_sender,
        })
    }

    /// Recompute `language` from the buffer path and contents (e.g. after Save As).

    #[cfg(feature = "timeline")]
    pub fn notify_timeline(&self, op: TimelineOp) {
        if let Some(sender) = &self.timeline_sender {
            sender.send_raw_event(TimelineEvent::Edit {
                op,
                cursor_line: self.cursor.line,
                cursor_col: self.cursor.col,
            });
        }
    }

    pub fn refresh_language(&mut self) {
        let path = self
            .buffer
            .file_path
            .as_deref()
            .unwrap_or(Path::new(""));
        self.language = crate::feature::language::detect_language(path, &self.buffer);
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        let pos = self.cursor.char_index(&self.buffer);
        let text = ch.to_string();

        let __cmd = EditCommand::Insert {
                pos,
                text: text.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.insert(pos, &text);

        // Move cursor after the inserted character
        if ch == '\n' {
            self.cursor.line += 1;
            self.cursor.col = 0;
        } else {
            self.cursor.col += 1;
        }
        self.cursor.col_target = self.cursor.col;
    }

    /// Insert a string at the cursor position.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let pos = self.cursor.char_index(&self.buffer);

        let __cmd = EditCommand::Insert {
                pos,
                text: text.to_string(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.insert(pos, text);

        // Update cursor position based on inserted text
        let newline_count = text.chars().filter(|&c| c == '\n').count();
        if newline_count > 0 {
            self.cursor.line += newline_count;
            // Column is the length of text after the last newline
            self.cursor.col = text
                .rsplit('\n')
                .next()
                .map_or(0, |s| s.chars().count());
        } else {
            self.cursor.col += text.chars().count();
        }
        self.cursor.col_target = self.cursor.col;
    }

    /// Delete the character before the cursor (Backspace).
    pub fn backspace(&mut self) {
        let pos = self.cursor.char_index(&self.buffer);
        if pos == 0 {
            return;
        }

        let deleted_char = self.buffer.rope.char(pos - 1);
        let deleted = deleted_char.to_string();

        let __cmd = EditCommand::Delete {
                pos: pos - 1,
                text: deleted,
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(pos - 1, pos);

        // Move cursor back
        if deleted_char == '\n' {
            self.cursor.line -= 1;
            self.cursor.col = self.buffer.line_len(self.cursor.line);
        } else {
            self.cursor.col = self.cursor.col.saturating_sub(1);
        }
        self.cursor.col_target = self.cursor.col;
    }

    /// Delete the character at the cursor (Delete key).
    pub fn delete_char(&mut self) {
        let pos = self.cursor.char_index(&self.buffer);
        if pos >= self.buffer.len_chars() {
            return;
        }

        let deleted_char = self.buffer.rope.char(pos);
        let deleted = deleted_char.to_string();

        let __cmd = EditCommand::Delete {
                pos,
                text: deleted,
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(pos, pos + 1);
        // Cursor position doesn't change for forward delete
    }

    /// Replace the last `prefix_len` characters before the cursor with `new_text`.
    /// Used for accepting a completion list item (one undo step).
    pub fn replace_before_cursor(&mut self, prefix_len: usize, new_text: &str) {
        let pos = self.cursor.char_index(&self.buffer);
        if prefix_len == 0 || prefix_len > pos {
            return;
        }
        let start = pos - prefix_len;
        let deleted = self.buffer.rope.slice(start..pos).to_string();
        let __cmd = EditCommand::Replace {
                pos: start,
                old_text: deleted,
                new_text: new_text.to_string(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );
        self.buffer.delete(start, pos);
        self.buffer.insert(start, new_text);
        let new_len = new_text.chars().count();
        self.cursor.col = self.cursor.col.saturating_sub(prefix_len) + new_len;
        self.cursor.col_target = self.cursor.col;
    }

    /// Delete the selected text, if any. Returns the deleted text.
    pub fn delete_selection(&mut self) -> Option<String> {
        let (start, end) = self.cursor.selection_range(&self.buffer)?;
        if start == end {
            self.cursor.clear_selection();
            return None;
        }

        let deleted = self.buffer.rope.slice(start..end).to_string();

        let __cmd = EditCommand::Delete {
                pos: start,
                text: deleted.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(start, end);

        // Move cursor to start of selection
        self.cursor.line = self.buffer.char_to_line(start);
        let line_start = self.buffer.line_to_char(self.cursor.line);
        self.cursor.col = start - line_start;
        self.cursor.col_target = self.cursor.col;
        self.cursor.clear_selection();

        Some(deleted)
    }

    /// Delete the current line (Ctrl+D).
    pub fn delete_line(&mut self) {
        let line = self.cursor.line;
        let line_start = self.buffer.line_to_char(line);
        let line_end = if line + 1 < self.buffer.line_count() {
            self.buffer.line_to_char(line + 1)
        } else {
            self.buffer.len_chars()
        };

        if line_start == line_end {
            return;
        }

        let text = self.buffer.rope.slice(line_start..line_end).to_string();

        let __cmd = EditCommand::Delete {
                pos: line_start,
                text,
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(line_start, line_end);
        self.cursor.clamp(&self.buffer);
    }

    /// Move a line up (Alt+Up).
    pub fn move_line_up(&mut self) {
        if self.cursor.line == 0 {
            return;
        }

        let current = self.cursor.line;
        let prev = current - 1;

        let curr_start = self.buffer.line_to_char(current);
        let curr_end = if current + 1 < self.buffer.line_count() {
            self.buffer.line_to_char(current + 1)
        } else {
            self.buffer.len_chars()
        };
        let current_text = self.buffer.rope.slice(curr_start..curr_end).to_string();

        let prev_start = self.buffer.line_to_char(prev);
        let prev_text = self.buffer.rope.slice(prev_start..curr_start).to_string();

        // Record as replace for undo
        let combined_old = format!("{}{}", prev_text, current_text);
        let combined_new = format!("{}{}", current_text, prev_text);

        let __cmd = EditCommand::Replace {
                pos: prev_start,
                old_text: combined_old.clone(),
                new_text: combined_new.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(prev_start, curr_end);
        self.buffer.insert(prev_start, &combined_new);
        self.cursor.line -= 1;
    }

    /// Move a line down (Alt+Down).
    pub fn move_line_down(&mut self) {
        if self.cursor.line >= self.buffer.line_count().saturating_sub(1) {
            return;
        }

        let current = self.cursor.line;
        let next = current + 1;

        let curr_start = self.buffer.line_to_char(current);
        let next_end = if next + 1 < self.buffer.line_count() {
            self.buffer.line_to_char(next + 1)
        } else {
            self.buffer.len_chars()
        };
        let next_start = self.buffer.line_to_char(next);

        let current_text = self.buffer.rope.slice(curr_start..next_start).to_string();
        let next_text = self.buffer.rope.slice(next_start..next_end).to_string();

        let combined_old = format!("{}{}", current_text, next_text);
        let combined_new = format!("{}{}", next_text, current_text);

        let __cmd = EditCommand::Replace {
                pos: curr_start,
                old_text: combined_old,
                new_text: combined_new.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(curr_start, next_end);
        self.buffer.insert(curr_start, &combined_new);
        self.cursor.line += 1;
    }

    /// Toggle line comment (Ctrl+/).
    pub fn toggle_comment(&mut self) {
        let comment_prefix = crate::feature::language::comment_prefix(&self.language);
        let line = self.cursor.line;
        let text = self.buffer.line_text(line);
        let trimmed = text.trim_start();

        let line_start = self.buffer.line_to_char(line);

        if trimmed.starts_with(&comment_prefix) {
            // Remove comment: find the prefix position and remove it
            let prefix_start = text.find(&comment_prefix).unwrap_or(0);
            let remove_len = if text[prefix_start + comment_prefix.len()..].starts_with(' ') {
                comment_prefix.len() + 1
            } else {
                comment_prefix.len()
            };

            let pos = line_start + prefix_start;
            let deleted_text = self
                .buffer
                .rope
                .slice(pos..pos + remove_len)
                .to_string();

            let __cmd = EditCommand::Delete {
                    pos,
                    text: deleted_text,
                };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );
            self.buffer.delete(pos, pos + remove_len);
        } else {
            // Add comment at the start of non-whitespace
            let indent_len = text.len() - trimmed.len();
            let pos = line_start + indent_len;
            let insert_text = format!("{} ", comment_prefix);

            let __cmd = EditCommand::Insert {
                    pos,
                    text: insert_text.clone(),
                };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );
            self.buffer.insert(pos, &insert_text);
        }
    }

    /// Indent the current line or selection (Tab).
    pub fn indent(&mut self, tab_size: usize) {
        let indent = " ".repeat(tab_size);
        let line = self.cursor.line;
        let pos = self.buffer.line_to_char(line);

        let __cmd = EditCommand::Insert {
                pos,
                text: indent.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.insert(pos, &indent);
        self.cursor.col += tab_size;
        self.cursor.col_target = self.cursor.col;
    }

    /// Dedent the current line (Shift+Tab).
    pub fn dedent(&mut self, tab_size: usize) {
        let line = self.cursor.line;
        let text = self.buffer.line_text(line);
        let pos = self.buffer.line_to_char(line);

        // Count leading spaces (up to tab_size)
        let leading_spaces = text.chars().take(tab_size).take_while(|&c| c == ' ').count();
        if leading_spaces == 0 {
            return;
        }

        let deleted = " ".repeat(leading_spaces);
        let __cmd = EditCommand::Delete {
                pos,
                text: deleted,
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );

        self.buffer.delete(pos, pos + leading_spaces);
        self.cursor.col = self.cursor.col.saturating_sub(leading_spaces);
        self.cursor.col_target = self.cursor.col;
    }

    /// Undo the last operation.
    pub fn undo(&mut self) {
        if let Some((cmd, line, col)) = self.history.undo() {
            match cmd {
                
                EditCommand::Insert { pos, text } => {
                    self.buffer
                        .delete(pos, pos + text.chars().count());
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Delete { pos, text });
                }
                EditCommand::Delete { pos, text } => {
                    self.buffer.insert(pos, &text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Insert { pos, text });
                }
                EditCommand::Replace {
                    pos,
                    old_text,
                    new_text,
                } => {
                    self.buffer
                        .delete(pos, pos + new_text.chars().count());
                    self.buffer.insert(pos, &old_text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Replace { pos, old_text: new_text, new_text: old_text });
                }

            }
            self.cursor.line = line;
            self.cursor.col = col;
            self.cursor.col_target = col;
            self.cursor.clear_selection();
            self.buffer.modified = true;
        }
    }

    /// Redo the last undone operation.
    pub fn redo(&mut self) {
        if let Some(cmd) = self.history.redo() {
            match cmd {

                EditCommand::Insert { pos, ref text } => {
                    self.buffer.insert(pos, text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Insert { pos: pos, text: text.clone() });
                    let end = pos + text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }
                EditCommand::Delete { pos, ref text } => {
                    self.buffer
                        .delete(pos, pos + text.chars().count());
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Delete { pos: pos, text: text.clone() });
                    self.cursor.line = self.buffer.char_to_line(pos);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = pos - line_start;
                }
                EditCommand::Replace {
                    pos,
                    ref old_text,
                    ref new_text,
                } => {
                    self.buffer
                        .delete(pos, pos + old_text.chars().count());
                    self.buffer.insert(pos, new_text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Replace { pos: pos, old_text: old_text.clone(), new_text: new_text.clone() });
                    let end = pos + new_text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }
            }
            self.cursor.col_target = self.cursor.col;
            self.buffer.modified = true;
        }
    }

    /// Replace the half-open character range `[start, end)` with `new_text` (one undo step).
    pub fn replace_char_range(&mut self, start: usize, end: usize, new_text: &str) -> bool {
        if start >= end || end > self.buffer.len_chars() {
            return false;
        }
        let old_text = self.buffer.rope.slice(start..end).to_string();
        let __cmd = EditCommand::Replace {
                pos: start,
                old_text,
                new_text: new_text.to_string(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );
        self.buffer.delete(start, end);
        self.buffer.insert(start, new_text);
        let new_len = new_text.chars().count();
        self.cursor.line = self.buffer.char_to_line(start);
        let line_start = self.buffer.line_to_char(self.cursor.line);
        self.cursor.col = start - line_start + new_len;
        self.cursor.col_target = self.cursor.col;
        self.cursor.clear_selection();
        self.buffer.modified = true;
        true
    }

    /// Duplicate the current line after itself (Ctrl+Shift+D).
    pub fn duplicate_line(&mut self) {
        let line = self.cursor.line;
        let line_start = self.buffer.line_to_char(line);
        let line_end = if line + 1 < self.buffer.line_count() {
            self.buffer.line_to_char(line + 1)
        } else {
            self.buffer.len_chars()
        };
        let text = self.buffer.rope.slice(line_start..line_end).to_string();
        let __cmd = EditCommand::Insert {
                pos: line_end,
                text: text.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );
        self.buffer.insert(line_end, &text);
        self.cursor.line += 1;
        self.cursor.col = 0;
        self.cursor.col_target = 0;
    }

    /// Replace all search matches in one undo step (`SearchMatch` uses char indices).
    pub fn replace_all_matches(&mut self, matches: &[SearchMatch], replacement: &str) -> usize {
        if matches.is_empty() {
            return 0;
        }
        let old_text = self.buffer.to_string();
        let mut chars: Vec<char> = old_text.chars().collect();
        for m in matches.iter().rev() {
            if m.start < m.end && m.end <= chars.len() {
                chars.splice(m.start..m.end, replacement.chars());
            }
        }
        let new_text: String = chars.into_iter().collect();
        if new_text == old_text {
            return 0;
        }
        let n = matches.len();
        let __cmd = EditCommand::Replace {
                pos: 0,
                old_text,
                new_text: new_text.clone(),
            };
        #[cfg(feature = "timeline")]
        {
            let op = match &__cmd {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            __cmd,
            self.cursor.line,
            self.cursor.col,
        );
        self.buffer.rope = ropey::Rope::from_str(&new_text);
        self.buffer.modified = true;
        self.cursor.clamp(&self.buffer);
        n
    }

    /// Save the document to disk.
    pub fn save(&mut self) -> Result<()> {
        self.buffer.save()
    }

    /// Save the document to a new path.
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        self.buffer.save_to(path)
    }

    /// Get the display name of the document.
    pub fn display_name(&self) -> String {
        self.buffer.display_name()
    }

    /// Check if the document has been modified.
    pub fn is_modified(&self) -> bool {
        self.buffer.modified
    }

    /// Ensure the cursor is visible by adjusting scroll offset.
    pub fn ensure_cursor_visible(&mut self, viewport_height: usize, viewport_width: usize) {
        // Vertical scrolling
        if self.cursor.line < self.scroll_y {
            self.scroll_y = self.cursor.line;
        } else if self.cursor.line >= self.scroll_y + viewport_height {
            self.scroll_y = self.cursor.line - viewport_height + 1;
        }

        // Horizontal scrolling
        if self.cursor.col < self.scroll_x {
            self.scroll_x = self.cursor.col;
        } else if self.cursor.col >= self.scroll_x + viewport_width {
            self.scroll_x = self.cursor.col - viewport_width + 1;
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_char() {
        let mut doc = Document::new();
        doc.insert_char('H');
        doc.insert_char('i');
        assert_eq!(doc.buffer.to_string(), "Hi");
        assert_eq!(doc.cursor.col, 2);
        assert!(doc.is_modified());
    }

    #[test]
    fn test_insert_newline() {
        let mut doc = Document::new();
        doc.insert_text("Hello");
        doc.insert_char('\n');
        doc.insert_text("World");

        assert_eq!(doc.buffer.to_string(), "Hello\nWorld");
        assert_eq!(doc.cursor.line, 1);
        assert_eq!(doc.cursor.col, 5);
    }

    #[test]
    fn test_backspace() {
        let mut doc = Document::new();
        doc.insert_text("Hello");
        doc.backspace();
        assert_eq!(doc.buffer.to_string(), "Hell");
        assert_eq!(doc.cursor.col, 4);
    }

    #[test]
    fn test_delete_char() {
        let mut doc = Document::new();
        doc.insert_text("Hello");
        doc.cursor.col = 0;
        doc.cursor.col_target = 0;
        doc.delete_char();
        assert_eq!(doc.buffer.to_string(), "ello");
    }

    #[test]
    fn test_undo_redo() {
        let mut doc = Document::new();
        doc.insert_text("Hello");
        assert_eq!(doc.buffer.to_string(), "Hello");

        doc.undo();
        assert_eq!(doc.buffer.to_string(), "");

        doc.redo();
        assert_eq!(doc.buffer.to_string(), "Hello");
    }

    #[test]
    fn test_delete_selection() {
        let mut doc = Document::new();
        doc.insert_text("Hello World");
        doc.cursor.col = 5;
        doc.cursor.col_target = 5;

        // Select " World"
        for _ in 0..6 {
            doc.cursor.select_right(&doc.buffer);
        }

        let deleted = doc.delete_selection();
        assert_eq!(deleted, Some(" World".to_string()));
        assert_eq!(doc.buffer.to_string(), "Hello");
    }

    #[test]
    fn test_modified_flag() {
        let mut doc = Document::new();
        assert!(!doc.is_modified());

        doc.insert_char('A');
        assert!(doc.is_modified());
    }

    #[test]
    fn test_ensure_cursor_visible() {
        let mut doc = Document::new();
        for i in 0..100 {
            doc.insert_text(&format!("Line {}\n", i));
        }
        doc.cursor.line = 50;
        doc.cursor.col = 0;

        doc.ensure_cursor_visible(20, 80);
        assert!(doc.scroll_y <= 50);
        assert!(doc.scroll_y + 20 > 50);
    }

    #[test]
    fn test_duplicate_line() {
        let mut doc = Document::new();
        doc.insert_text("foo\nbar\n");
        doc.cursor.line = 0;
        doc.duplicate_line();
        assert_eq!(doc.buffer.to_string(), "foo\nfoo\nbar\n");
        assert_eq!(doc.cursor.line, 1);
    }

    #[test]
    fn test_replace_all_matches() {
        let mut doc = Document::new();
        doc.insert_text("a b a");
        let matches = vec![
            SearchMatch {
                start: 0,
                end: 1,
                text: "a".into(),
            },
            SearchMatch {
                start: 4,
                end: 5,
                text: "a".into(),
            },
        ];
        let n = doc.replace_all_matches(&matches, "Z");
        assert_eq!(n, 2);
        assert_eq!(doc.buffer.to_string(), "Z b Z");
    }
}
