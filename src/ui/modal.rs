/// Modal overlay system for find bar, go-to-line, and other dialogs.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::theme::Theme;

/// The type of modal being displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalKind {
    /// Find bar (Ctrl+F).
    Find,
    /// Find and replace (Ctrl+H).
    FindReplace,
    /// Go to line (Ctrl+G).
    GoToLine,
    /// Save confirmation when closing modified buffer.
    SaveConfirm,
}

/// Modal state.
#[derive(Debug, Clone)]
pub struct ModalState {
    pub kind: ModalKind,
    pub input: String,
    pub replace_input: String,
    pub cursor_pos: usize,
    pub message: Option<String>,
}

impl ModalState {
    /// Create a new find modal.
    pub fn find() -> Self {
        Self {
            kind: ModalKind::Find,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            message: None,
        }
    }

    /// Create a new find-replace modal.
    pub fn find_replace() -> Self {
        Self {
            kind: ModalKind::FindReplace,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            message: None,
        }
    }

    /// Create a go-to-line modal.
    pub fn goto_line() -> Self {
        Self {
            kind: ModalKind::GoToLine,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            message: None,
        }
    }

    /// Create a save confirmation modal.
    pub fn save_confirm(filename: &str) -> Self {
        Self {
            kind: ModalKind::SaveConfirm,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            message: Some(format!(
                "Save changes to '{}'? (y) Save (n) Don't save (Esc) Cancel",
                filename
            )),
        }
    }

    /// Insert a character into the input.
    pub fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor_pos, ch);
        self.cursor_pos += 1;
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.input.remove(self.cursor_pos);
        }
    }

    /// Move cursor left.
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right.
    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.input.len() {
            self.cursor_pos += 1;
        }
    }

    /// Get the input text.
    pub fn text(&self) -> &str {
        &self.input
    }
}

/// Modal widget renderer.
pub struct ModalWidget<'a> {
    state: &'a ModalState,
    theme: &'a Theme,
    search_status: Option<String>,
}

impl<'a> ModalWidget<'a> {
    /// Create a new modal widget.
    pub fn new(state: &'a ModalState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            search_status: None,
        }
    }

    /// Set the search status text.
    pub fn search_status(mut self, status: Option<String>) -> Self {
        self.search_status = status;
        self
    }
}

impl<'a> Widget for ModalWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.height == 0 || area.width < 20 {
            return;
        }

        // Modal appears at the top of the editor area
        let modal_width = area.width.min(80);
        let modal_x = area.x + (area.width - modal_width) / 2;
        let modal_y = area.y;

        let border_style = Style::default()
            .fg(self.theme.ui.panel_border)
            .bg(self.theme.editor.background);

        let input_style = Style::default()
            .fg(self.theme.editor.foreground)
            .bg(self.theme.editor.background);

        let label_style = Style::default()
            .fg(self.theme.ui.status_bar_bg)
            .bg(self.theme.editor.background)
            .add_modifier(Modifier::BOLD);

        let height = match self.state.kind {
            ModalKind::Find => 1,
            ModalKind::FindReplace => 2,
            ModalKind::GoToLine => 1,
            ModalKind::SaveConfirm => 1,
        };

        // Draw background
        for row in 0..height {
            for x in modal_x..modal_x + modal_width {
                buf.set_string(x, modal_y + row, " ", input_style);
            }
        }

        match self.state.kind {
            ModalKind::Find => {
                let label = "Find: ";
                buf.set_string(modal_x, modal_y, label, label_style);
                buf.set_string(
                    modal_x + label.len() as u16,
                    modal_y,
                    &self.state.input,
                    input_style,
                );

                if let Some(ref status) = self.search_status {
                    let status_x = modal_x + modal_width - status.len() as u16 - 1;
                    buf.set_string(status_x, modal_y, status, border_style);
                }

                // Cursor
                let cursor_x = modal_x + label.len() as u16 + self.state.cursor_pos as u16;
                if cursor_x < modal_x + modal_width {
                    let cursor_style = Style::default()
                        .bg(self.theme.editor.cursor)
                        .fg(self.theme.editor.background);
                    let ch = self
                        .state
                        .input
                        .chars()
                        .nth(self.state.cursor_pos)
                        .map_or(" ".to_string(), |c| c.to_string());
                    buf.set_string(cursor_x, modal_y, ch, cursor_style);
                }
            }
            ModalKind::FindReplace => {
                let label1 = "Find:    ";
                let label2 = "Replace: ";
                buf.set_string(modal_x, modal_y, label1, label_style);
                buf.set_string(
                    modal_x + label1.len() as u16,
                    modal_y,
                    &self.state.input,
                    input_style,
                );
                buf.set_string(modal_x, modal_y + 1, label2, label_style);
                buf.set_string(
                    modal_x + label2.len() as u16,
                    modal_y + 1,
                    &self.state.replace_input,
                    input_style,
                );

                if let Some(ref status) = self.search_status {
                    let status_x = modal_x + modal_width - status.len() as u16 - 1;
                    buf.set_string(status_x, modal_y, status, border_style);
                }
            }
            ModalKind::GoToLine => {
                let label = "Go to Line: ";
                buf.set_string(modal_x, modal_y, label, label_style);
                buf.set_string(
                    modal_x + label.len() as u16,
                    modal_y,
                    &self.state.input,
                    input_style,
                );

                // Cursor
                let cursor_x = modal_x + label.len() as u16 + self.state.cursor_pos as u16;
                if cursor_x < modal_x + modal_width {
                    let cursor_style = Style::default()
                        .bg(self.theme.editor.cursor)
                        .fg(self.theme.editor.background);
                    let ch = self
                        .state
                        .input
                        .chars()
                        .nth(self.state.cursor_pos)
                        .map_or(" ".to_string(), |c| c.to_string());
                    buf.set_string(cursor_x, modal_y, ch, cursor_style);
                }
            }
            ModalKind::SaveConfirm => {
                if let Some(ref msg) = self.state.message {
                    buf.set_string(modal_x, modal_y, msg, label_style);
                }
            }
        }
    }
}
