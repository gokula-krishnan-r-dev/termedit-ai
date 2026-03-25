//! Filterable Go to Symbol overlay (Ctrl/Cmd+Shift+O).

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::theme::Theme;
use crate::feature::outline::OutlineSymbol;

/// Snapshot + filter state for the symbol picker.
#[derive(Debug, Clone)]
pub struct OutlinePaletteState {
    pub visible: bool,
    pub filter: String,
    pub selected: usize,
    /// Indices into `symbols` from the last [`rebuild_filtered`](OutlinePaletteState::rebuild_filtered).
    pub filtered_indices: Vec<usize>,
    /// Symbols at open time (active buffer).
    pub symbols: Vec<OutlineSymbol>,
    /// Optional footer line (huge file, disabled hint).
    pub hint: Option<String>,
    /// Title (e.g. "Go to Symbol" or error summary).
    pub title: String,
    /// Scroll offset into filtered list.
    pub scroll: usize,
}

impl OutlinePaletteState {
    pub fn new() -> Self {
        Self {
            visible: false,
            filter: String::new(),
            selected: 0,
            filtered_indices: Vec::new(),
            symbols: Vec::new(),
            hint: None,
            title: "Go to Symbol".to_string(),
            scroll: 0,
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.selected = 0;
        self.filtered_indices.clear();
        self.symbols.clear();
        self.hint = None;
        self.title = "Go to Symbol".to_string();
        self.scroll = 0;
    }

    pub fn open_into(&mut self, symbols: Vec<OutlineSymbol>, hint: Option<String>, title: Option<String>) {
        self.visible = true;
        self.filter.clear();
        self.selected = 0;
        self.scroll = 0;
        self.symbols = symbols;
        self.hint = hint;
        self.title = title.unwrap_or_else(|| "Go to Symbol".to_string());
        self.rebuild_filtered();
    }

    pub fn rebuild_filtered(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered_indices.clear();
        for (i, sym) in self.symbols.iter().enumerate() {
            let hay = format!(
                "{} {} {}",
                sym.kind.prefix(),
                sym.name,
                sym.name.to_lowercase()
            );
            if q.is_empty() || hay.to_lowercase().contains(&q) {
                self.filtered_indices.push(i);
            }
        }
        self.selected = self.selected.min(self.filtered_indices.len().saturating_sub(1));
        self.scroll = 0;
    }

    pub fn selected_symbol(&self) -> Option<&OutlineSymbol> {
        let idx = self.filtered_indices.get(self.selected).copied()?;
        self.symbols.get(idx)
    }

    pub fn move_selection(&mut self, delta: isize, visible_rows: usize) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let max_sel = self.filtered_indices.len() - 1;
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

impl Default for OutlinePaletteState {
    fn default() -> Self {
        Self::new()
    }
}

/// Ratatui overlay for Go to Symbol.
pub struct OutlinePaletteWidget<'a> {
    pub state: &'a OutlinePaletteState,
    pub theme: &'a Theme,
}

impl<'a> Widget for OutlinePaletteWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.width < 20 || area.height < 5 || !self.state.visible {
            return;
        }

        let w = (area.width as usize).min(60).max(30) as u16;
        let h = (area.height / 2).min(16).max(8) as u16;
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

        let filter_line = if self.state.filter.is_empty() {
            "> ".to_string()
        } else {
            format!("> {}", self.state.filter)
        };
        buf.set_string(x + 1, y + 1, &filter_line, normal);

        let footer_reserve = if self.state.hint.is_some() { 2u16 } else { 1u16 };
        let max_rows = h.saturating_sub(2 + footer_reserve);
        let visible = max_rows as usize;

        let total = self.state.filtered_indices.len();
        for row_i in 0..visible {
            let sym_idx = self.state.scroll + row_i;
            if sym_idx >= total {
                break;
            }
            let Some(&orig_i) = self.state.filtered_indices.get(sym_idx) else {
                break;
            };
            let Some(sym) = self.state.symbols.get(orig_i) else {
                break;
            };
            let line = format!(
                "  {:<8} {}",
                sym.kind.prefix(),
                sym.name
            );
            let line_trim = if line.len() > w as usize - 2 {
                format!("{}…", &line[..(w as usize).saturating_sub(3).min(line.len())])
            } else {
                line
            };
            let st = if sym_idx == self.state.selected { hi } else { normal };
            buf.set_string(x + 1, y + 2 + row_i as u16, &line_trim, st);
        }

        if let Some(ref hint) = self.state.hint {
            let hint_line = if hint.len() + 2 > w as usize {
                format!(" {}", &hint[..hint.len().min(w as usize - 3)])
            } else {
                format!(" {}", hint)
            };
            buf.set_string(x + 1, y + h - 2, &hint_line, dim);
        }

        let foot = "↑↓ Enter Esc · filter by name";
        let foot_trim = if foot.len() > w as usize - 2 {
            &foot[..(w as usize).saturating_sub(3).min(foot.len())]
        } else {
            foot
        };
        buf.set_string(x + 1, y + h - 1, foot_trim, border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::outline::{OutlineSymbol, SymbolKind};

    #[test]
    fn filter_symbols() {
        let mut s = OutlinePaletteState::new();
        s.symbols = vec![
            OutlineSymbol {
                kind: SymbolKind::Function,
                name: "alpha".into(),
                start_line: 0,
                name_start_col: 0,
            },
            OutlineSymbol {
                kind: SymbolKind::Struct,
                name: "BetaThing".into(),
                start_line: 1,
                name_start_col: 0,
            },
        ];
        s.visible = true;
        s.filter = "beta".into();
        s.rebuild_filtered();
        assert_eq!(s.filtered_indices.len(), 1);
        assert_eq!(s.symbols[s.filtered_indices[0]].name, "BetaThing");
    }
}
