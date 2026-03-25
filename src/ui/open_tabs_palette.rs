//! Find in Open Tabs overlay (Ctrl/Cmd+Shift+F).

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::theme::Theme;
use crate::feature::search::OpenTabHit;

/// State for the open-tabs search picker.
#[derive(Debug, Clone)]
pub struct OpenTabsPaletteState {
    pub visible: bool,
    pub query: String,
    pub hits: Vec<OpenTabHit>,
    pub selected: usize,
    pub scroll: usize,
    pub hint: Option<String>,
    pub last_error: Option<String>,
    pub title: String,
}

impl OpenTabsPaletteState {
    pub const TITLE: &'static str = "Find in Open Tabs";

    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            hits: Vec::new(),
            selected: 0,
            scroll: 0,
            hint: None,
            last_error: None,
            title: Self::TITLE.to_string(),
        }
    }

    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.hits.clear();
        self.selected = 0;
        self.scroll = 0;
        self.hint = None;
        self.last_error = None;
        self.title = Self::TITLE.to_string();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.hits.clear();
        self.selected = 0;
        self.scroll = 0;
        self.hint = None;
        self.last_error = None;
        self.title = Self::TITLE.to_string();
    }

    pub fn selected_hit(&self) -> Option<&OpenTabHit> {
        self.hits.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize, visible_rows: usize) {
        if self.hits.is_empty() {
            return;
        }
        let max_sel = self.hits.len() - 1;
        let new_sel = if delta < 0 {
            self.selected.saturating_sub((-delta) as usize)
        } else {
            (self.selected + delta as usize).min(max_sel)
        };
        self.selected = new_sel;
        let vr = visible_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + vr {
            self.scroll = self.selected + 1 - vr;
        }
    }
}

impl Default for OpenTabsPaletteState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OpenTabsPaletteWidget<'a> {
    pub state: &'a OpenTabsPaletteState,
    pub theme: &'a Theme,
}

impl<'a> Widget for OpenTabsPaletteWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.width < 20 || area.height < 5 || !self.state.visible {
            return;
        }

        let w = (area.width as usize).min(72).max(34) as u16;
        let h = (area.height / 2).min(18).max(9) as u16;
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;

        let border = Style::default()
            .fg(self.theme.ui.panel_border)
            .bg(self.theme.editor.background);
        let title_st = Style::default()
            .fg(self.theme.ui.status_bar_bg)
            .bg(self.theme.editor.background)
            .add_modifier(Modifier::BOLD);
        let normal = Style::default()
            .fg(self.theme.editor.foreground)
            .bg(self.theme.editor.background);
        let hi = Style::default()
            .fg(self.theme.editor.background)
            .bg(self.theme.ui.status_bar_bg);
        let dim = Style::default()
            .fg(self.theme.editor.line_number)
            .bg(self.theme.editor.background);
        let err_st = Style::default()
            .fg(self.theme.ui.find_toggle_on_bg)
            .bg(self.theme.editor.background)
            .add_modifier(Modifier::BOLD);

        for row in 0..h {
            for col in 0..w {
                buf.set_string(x + col, y + row, " ", normal);
            }
        }

        let title_line = if self.state.title.len() + 2 > w as usize {
            format!(" {}", &self.state.title[..self.state.title.len().min(w as usize - 3)])
        } else {
            format!(" {}", &self.state.title)
        };
        buf.set_string(x + 1, y, &title_line, title_st);

        let q_line = if self.state.query.is_empty() {
            "> ".to_string()
        } else {
            format!("> {}", self.state.query)
        };
        buf.set_string(x + 1, y + 1, &q_line, normal);

        let mut extra_footer: u16 = 0;
        if self.state.last_error.is_some() {
            extra_footer += 1;
        }
        if self.state.hint.is_some() {
            extra_footer += 1;
        }
        let footer_lines = 1 + extra_footer;
        let max_rows = h.saturating_sub(2 + footer_lines);
        let visible = max_rows as usize;

        let total = self.state.hits.len();
        for row_i in 0..visible {
            let idx = self.state.scroll + row_i;
            if idx >= total {
                break;
            }
            let Some(hit) = self.state.hits.get(idx) else {
                break;
            };
            let line_no = hit.line + 1;
            let line = format!("  {}:{}  {}", hit.tab_label, line_no, hit.preview);
            let line_trim = if line.len() > w as usize - 2 {
                format!("{}…", &line[..(w as usize).saturating_sub(3).min(line.len())])
            } else {
                line
            };
            let st = if idx == self.state.selected { hi } else { normal };
            buf.set_string(x + 1, y + 2 + row_i as u16, &line_trim, st);
        }

        let mut foot_row = h.saturating_sub(footer_lines);
        if let Some(ref err) = self.state.last_error {
            let s = if err.len() + 2 > w as usize {
                format!(" {}", &err[..err.len().min(w as usize - 3)])
            } else {
                format!(" {}", err)
            };
            buf.set_string(x + 1, y + foot_row, &s, err_st);
            foot_row = foot_row.saturating_add(1);
        }
        if let Some(ref hint) = self.state.hint {
            let s = if hint.len() + 2 > w as usize {
                format!(" {}", &hint[..hint.len().min(w as usize - 3)])
            } else {
                format!(" {}", hint)
            };
            buf.set_string(x + 1, y + foot_row, &s, dim);
        }

        let foot = "↑↓ Enter Esc · search debounced";
        let foot_trim = if foot.len() > w as usize - 2 {
            &foot[..(w as usize).saturating_sub(3).min(foot.len())]
        } else {
            foot
        };
        buf.set_string(x + 1, y + h - 1, foot_trim, border);
    }
}
