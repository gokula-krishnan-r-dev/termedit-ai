/// File tree sidebar (placeholder for MVP).
///
/// Full implementation with expand/collapse will come in a later milestone.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::theme::Theme;

/// The file tree sidebar widget.
pub struct FileTree<'a> {
    theme: &'a Theme,
    visible: bool,
}

impl<'a> FileTree<'a> {
    /// Create a new file tree.
    pub fn new(theme: &'a Theme, visible: bool) -> Self {
        Self { theme, visible }
    }

    /// Whether the tree is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Suggested width for the sidebar.
    pub fn width() -> u16 {
        25
    }
}

impl<'a> Widget for FileTree<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if !self.visible || area.width == 0 || area.height == 0 {
            return;
        }

        let bg_style = Style::default()
            .fg(self.theme.editor.foreground)
            .bg(self.theme.editor.background);

        let header_style = Style::default()
            .fg(self.theme.ui.tab_fg)
            .bg(self.theme.ui.tab_inactive_bg)
            .add_modifier(Modifier::BOLD);

        // Fill background
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf.set_string(x, y, " ", bg_style);
            }
        }

        // Header
        let header = " EXPLORER";
        buf.set_string(area.x, area.y, header, header_style);
        for x in (area.x + header.len() as u16)..area.x + area.width {
            buf.set_string(x, area.y, " ", header_style);
        }

        // Placeholder message
        let msg = " No folder open";
        if area.height > 2 {
            let dim_style = Style::default()
                .fg(self.theme.editor.line_number)
                .bg(self.theme.editor.background);
            buf.set_string(area.x, area.y + 2, msg, dim_style);
        }

        // Right border
        let border_style = Style::default()
            .fg(self.theme.ui.panel_border)
            .bg(self.theme.editor.background);
        for y in area.y..area.y + area.height {
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
        }
    }
}
