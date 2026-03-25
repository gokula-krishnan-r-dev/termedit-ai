/// Undo/redo history using the command pattern.
///
/// Supports automatic coalescing of consecutive single-character
/// edits for a natural undo experience.

use std::time::Instant;

/// A single edit operation that can be undone/redone.
#[derive(Debug, Clone)]
pub enum EditCommand {
    /// Insert text at a character position.
    Insert {
        /// Character position where text was inserted.
        pos: usize,
        /// The text that was inserted.
        text: String,
    },
    /// Delete text from a range.
    Delete {
        /// Character position where deletion started.
        pos: usize,
        /// The text that was deleted (stored for undo).
        text: String,
    },
    /// Replace text (used for find-and-replace, etc.).
    Replace {
        /// Character position of the replacement.
        pos: usize,
        /// Original text before replacement.
        old_text: String,
        /// The replacement text.
        new_text: String,
    },
}

/// A group of edit commands treated as a single undo step.
#[derive(Debug, Clone)]
struct HistoryEntry {
    /// The edit command(s) in this entry.
    command: EditCommand,
    /// Timestamp for coalescing logic.
    timestamp: Instant,
    /// Cursor position before the edit.
    cursor_line: usize,
    cursor_col: usize,
}

/// Undo/redo history manager.
///
/// Maintains two stacks and supports coalescing consecutive
/// single-character insertions/deletions within a time window.
pub struct History {
    /// Stack of undoable operations.
    undo_stack: Vec<HistoryEntry>,
    /// Stack of redoable operations (cleared on new edit).
    redo_stack: Vec<HistoryEntry>,
    /// Maximum number of history entries.
    max_entries: usize,
    /// Time window for coalescing consecutive edits (milliseconds).
    coalesce_ms: u128,
}

impl History {
    /// Create a new history with default limits.
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::with_capacity(256),
            redo_stack: Vec::with_capacity(64),
            max_entries: 1000,
            coalesce_ms: 500,
        }
    }

    /// Record a new edit command, clearing the redo stack.
    pub fn record(
        &mut self,
        command: EditCommand,
        cursor_line: usize,
        cursor_col: usize,
    ) {
        // Clear redo stack on new edit
        self.redo_stack.clear();

        let now = Instant::now();

        // Try to coalesce with the last entry
        if let Some(last) = self.undo_stack.last_mut() {
            let elapsed = now.duration_since(last.timestamp).as_millis();
            if elapsed < self.coalesce_ms {
                if let Some(merged) = Self::try_coalesce(&last.command, &command) {
                    last.command = merged;
                    last.timestamp = now;
                    return;
                }
            }
        }

        // Push as a new entry
        self.undo_stack.push(HistoryEntry {
            command,
            timestamp: now,
            cursor_line,
            cursor_col,
        });

        // Enforce max entries limit
        if self.undo_stack.len() > self.max_entries {
            self.undo_stack.remove(0);
        }
    }

    /// Try to merge two consecutive edit commands.
    ///
    /// Only merges consecutive single-char inserts or deletes at adjacent positions.
    fn try_coalesce(prev: &EditCommand, next: &EditCommand) -> Option<EditCommand> {
        match (prev, next) {
            // Coalesce consecutive inserts at adjacent positions
            (
                EditCommand::Insert {
                    pos: p1,
                    text: t1,
                },
                EditCommand::Insert {
                    pos: p2,
                    text: t2,
                },
            ) => {
                // Only coalesce single characters
                if t2.len() <= 1 && *p2 == *p1 + t1.len() {
                    // Check for word boundary — break on whitespace after non-whitespace
                    let last_prev = t1.chars().last();
                    let first_next = t2.chars().next();
                    if let (Some(lp), Some(fn_)) = (last_prev, first_next) {
                        if !lp.is_whitespace() && fn_.is_whitespace() {
                            return None; // Word boundary, don't coalesce
                        }
                    }

                    Some(EditCommand::Insert {
                        pos: *p1,
                        text: format!("{}{}", t1, t2),
                    })
                } else {
                    None
                }
            }
            // Coalesce consecutive backspace deletes
            (
                EditCommand::Delete {
                    pos: p1,
                    text: t1,
                },
                EditCommand::Delete {
                    pos: p2,
                    text: t2,
                },
            ) => {
                if t2.len() <= 1 && *p2 + t2.len() == *p1 {
                    // Backspace: delete before previous
                    Some(EditCommand::Delete {
                        pos: *p2,
                        text: format!("{}{}", t2, t1),
                    })
                } else if t2.len() <= 1 && *p2 == *p1 {
                    // Forward delete: delete at same position
                    Some(EditCommand::Delete {
                        pos: *p1,
                        text: format!("{}{}", t1, t2),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Undo the last operation. Returns the command and cursor position to restore.
    pub fn undo(&mut self) -> Option<(EditCommand, usize, usize)> {
        let entry = self.undo_stack.pop()?;
        let restore_line = entry.cursor_line;
        let restore_col = entry.cursor_col;

        // Push the inverse to redo stack
        self.redo_stack.push(entry.clone());

        Some((entry.command, restore_line, restore_col))
    }

    /// Redo the last undone operation. Returns the command.
    pub fn redo(&mut self) -> Option<EditCommand> {
        let entry = self.redo_stack.pop()?;
        self.undo_stack.push(entry.clone());
        Some(entry.command)
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Get the number of undo entries.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_undo() {
        let mut hist = History::new();
        hist.record(
            EditCommand::Insert {
                pos: 0,
                text: "Hello".to_string(),
            },
            0,
            0,
        );
        assert!(hist.can_undo());

        let (cmd, line, col) = hist.undo().unwrap();
        match cmd {
            EditCommand::Insert { pos, text } => {
                assert_eq!(pos, 0);
                assert_eq!(text, "Hello");
            }
            _ => panic!("Expected Insert command"),
        }
        assert_eq!(line, 0);
        assert_eq!(col, 0);
    }

    #[test]
    fn test_undo_redo() {
        let mut hist = History::new();
        hist.record(
            EditCommand::Insert {
                pos: 0,
                text: "A".to_string(),
            },
            0,
            0,
        );

        // Undo
        hist.undo();
        assert!(hist.can_redo());
        assert!(!hist.can_undo());

        // Redo
        let cmd = hist.redo().unwrap();
        match cmd {
            EditCommand::Insert { text, .. } => assert_eq!(text, "A"),
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn test_redo_cleared_on_new_edit() {
        let mut hist = History::new();
        hist.record(
            EditCommand::Insert {
                pos: 0,
                text: "A".to_string(),
            },
            0,
            0,
        );
        hist.undo();
        assert!(hist.can_redo());

        // New edit should clear redo
        hist.record(
            EditCommand::Insert {
                pos: 0,
                text: "B".to_string(),
            },
            0,
            0,
        );
        assert!(!hist.can_redo());
    }

    #[test]
    fn test_empty_history() {
        let hist = History::new();
        assert!(!hist.can_undo());
        assert!(!hist.can_redo());
    }

    #[test]
    fn test_coalescing_inserts() {
        let mut hist = History::new();
        // Rapid single-char inserts should coalesce
        hist.record(
            EditCommand::Insert {
                pos: 0,
                text: "H".to_string(),
            },
            0,
            0,
        );
        hist.record(
            EditCommand::Insert {
                pos: 1,
                text: "i".to_string(),
            },
            0,
            1,
        );

        // Should have coalesced into one entry
        assert_eq!(hist.undo_count(), 1);

        let (cmd, _, _) = hist.undo().unwrap();
        match cmd {
            EditCommand::Insert { text, .. } => assert_eq!(text, "Hi"),
            _ => panic!("Expected coalesced Insert"),
        }
    }
}
