/// Tab bar — renders open document tabs at the top of the editor.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::theme::Theme;

/// Information about a single tab.
pub struct TabInfo {
    pub name: String,
    pub modified: bool,
}

/// The tab bar widget.
pub struct TabBar<'a> {
    tabs: &'a [TabInfo],
    active: usize,
    theme: &'a Theme,
}

impl<'a> TabBar<'a> {
    /// Create a new tab bar.
    pub fn new(tabs: &'a [TabInfo], active: usize, theme: &'a Theme) -> Self {
        Self {
            tabs,
            active,
            theme,
        }
    }
}

impl<'a> Widget for TabBar<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.height == 0 || self.tabs.is_empty() {
            return;
        }

        // Fill background
        let bg_style = Style::default().bg(self.theme.ui.tab_inactive_bg);
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", bg_style);
        }

        let mut x_offset = area.x;

        for (i, tab) in self.tabs.iter().enumerate() {
            let modified = if tab.modified { " ●" } else { "" };
            let label = format!(" {}{} ", tab.name, modified);
            let tab_width = label.len() as u16;

            if x_offset + tab_width > area.x + area.width {
                break; // Truncate if too many tabs
            }

            let style = if i == self.active {
                Style::default()
                    .fg(self.theme.ui.tab_fg)
                    .bg(self.theme.ui.tab_active_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(self.theme.ui.tab_fg)
                    .bg(self.theme.ui.tab_inactive_bg)
            };

            buf.set_string(x_offset, area.y, &label, style);

            // Separator
            if i < self.tabs.len() - 1 {
                let sep_style = Style::default()
                    .fg(self.theme.ui.panel_border)
                    .bg(self.theme.ui.tab_inactive_bg);
                buf.set_string(x_offset + tab_width, area.y, "│", sep_style);
                x_offset += tab_width + 1;
            } else {
                x_offset += tab_width;
            }
        }
    }
}

/// Determine which tab was clicked given an x coordinate.
pub fn tab_at_position(tabs: &[TabInfo], x: u16, area_x: u16) -> Option<usize> {
    let mut offset = area_x;
    for (i, tab) in tabs.iter().enumerate() {
        let modified = if tab.modified { " ●" } else { "" };
        let width = (tab.name.len() + modified.len() + 2) as u16; // " name● "
        if x >= offset && x < offset + width {
            return Some(i);
        }
        offset += width + 1; // +1 for separator
    }
    None
}
