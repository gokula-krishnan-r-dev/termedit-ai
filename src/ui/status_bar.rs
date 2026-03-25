/// Status bar — VS Code-style bottom information bar.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::Style,
    widgets::Widget,
};

use crate::config::theme::Theme;
use crate::core::document::Document;
use crate::feature::language;

/// The status bar widget.
pub struct StatusBar<'a> {
    doc: &'a Document,
    theme: &'a Theme,
    search_status: Option<String>,
    message: Option<String>,
    /// When set, shows `Tab {current}/{total}` (1-based index) for multi-tab sessions.
    tab_hint: Option<(usize, usize)>,
}

impl<'a> StatusBar<'a> {
    /// Create a new status bar.
    pub fn new(doc: &'a Document, theme: &'a Theme) -> Self {
        Self {
            doc,
            theme,
            search_status: None,
            message: None,
            tab_hint: None,
        }
    }

    /// Show active tab position when multiple buffers are open.
    pub fn tab_hint(mut self, current: usize, total: usize) -> Self {
        self.tab_hint = Some((current, total));
        self
    }

    /// Set a search status string (e.g., "3 of 12").
    pub fn search_status(mut self, status: Option<String>) -> Self {
        self.search_status = status;
        self
    }

    /// Set a temporary message to display.
    pub fn message(mut self, msg: Option<String>) -> Self {
        self.message = msg;
        self
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.height == 0 {
            return;
        }

        let style = Style::default()
            .fg(self.theme.ui.status_bar_fg)
            .bg(self.theme.ui.status_bar_bg);

        // Fill background
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", style);
        }

        // Left section: modified indicator + filename
        let modified = if self.doc.is_modified() { " ●" } else { "" };
        let filename = self.doc.display_name();
        let left = format!(" {}{}", filename, modified);

        // Center section: message or search status
        let center = if let Some(ref msg) = self.message {
            msg.clone()
        } else if let Some(ref status) = self.search_status {
            format!("Search: {}", status)
        } else {
            String::new()
        };

        // Right section: language, encoding, line ending, cursor position
        let lang = language::display_name(&self.doc.language);
        let encoding = self.doc.buffer.encoding.to_string();
        let line_ending = self.doc.buffer.line_ending.to_string();
        let position = format!(
            "Ln {}, Col {}",
            self.doc.cursor.line + 1,
            self.doc.cursor.col + 1
        );
        let tab_part = self
            .tab_hint
            .map(|(i, n)| format!("Tab {}/{}  ", i, n))
            .unwrap_or_default();
        let right = format!(
            "{}{}  {}  {}  {} ",
            tab_part, lang, encoding, line_ending, position
        );

        // Render sections
        let width = area.width as usize;

        // Left
        let left_truncated = if left.len() > width / 3 {
            &left[..width / 3]
        } else {
            &left
        };
        buf.set_string(area.x, area.y, left_truncated, style);

        // Center
        if !center.is_empty() {
            let center_x = area.x + (width / 2).saturating_sub(center.len() / 2) as u16;
            buf.set_string(center_x, area.y, &center, style);
        }

        // Right
        let right_x = (area.x + area.width).saturating_sub(right.len() as u16);
        if right_x > area.x + left_truncated.len() as u16 {
            buf.set_string(right_x, area.y, &right, style);
        }
    }
}
