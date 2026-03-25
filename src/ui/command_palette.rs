//! Command palette (Ctrl+P): filterable list of editor commands.
//! Maintainer notes: see `docs/DEVELOPER.md`.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::keymap::Action;
use crate::config::theme::Theme;

/// Built-in palette commands. Extend this enum and `labels()` to add entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCmd {
    NewFile,
    Open,
    Save,
    SaveAs,
    CloseTab,
    Quit,
    Find,
    FindReplace,
    FindInOpenTabs,
    AiAssistant,
    AiBrainstormIdeas,
    GotoLine,
    GoToSymbol,
    GoToMatchingBracket,
    ToggleFileTree,
    NextTab,
    PrevTab,
}

impl PaletteCmd {
    /// Human-readable label shown in the palette (also used for filtering).
    pub fn label(self) -> &'static str {
        match self {
            PaletteCmd::NewFile => "New File",
            PaletteCmd::Open => "Open File…",
            PaletteCmd::Save => "Save",
            PaletteCmd::SaveAs => "Save As…",
            PaletteCmd::CloseTab => "Close Tab",
            PaletteCmd::Quit => "Quit",
            PaletteCmd::Find => "Find",
            PaletteCmd::FindReplace => "Find and Replace",
            PaletteCmd::FindInOpenTabs => "Find in Open Tabs…",
            PaletteCmd::AiAssistant => "AI Assistant…",
            PaletteCmd::AiBrainstormIdeas => "AI: Brainstorm ideas…",
            PaletteCmd::GotoLine => "Go to Line",
            PaletteCmd::GoToSymbol => "Go to Symbol…",
            PaletteCmd::GoToMatchingBracket => "Go to Matching Bracket",
            PaletteCmd::ToggleFileTree => "Toggle File Tree",
            PaletteCmd::NextTab => "Next Tab",
            PaletteCmd::PrevTab => "Previous Tab",
        }
    }

    /// All commands in display order.
    pub fn all() -> &'static [PaletteCmd] {
        &[
            PaletteCmd::NewFile,
            PaletteCmd::Open,
            PaletteCmd::Save,
            PaletteCmd::SaveAs,
            PaletteCmd::CloseTab,
            PaletteCmd::Find,
            PaletteCmd::FindReplace,
            PaletteCmd::FindInOpenTabs,
            PaletteCmd::AiAssistant,
            PaletteCmd::AiBrainstormIdeas,
            PaletteCmd::GotoLine,
            PaletteCmd::GoToSymbol,
            PaletteCmd::GoToMatchingBracket,
            PaletteCmd::ToggleFileTree,
            PaletteCmd::NextTab,
            PaletteCmd::PrevTab,
            PaletteCmd::Quit,
        ]
    }

    /// Map to a keymap [`Action`] where one exists.
    pub fn to_action(self) -> Option<Action> {
        match self {
            PaletteCmd::NewFile => Some(Action::NewFile),
            PaletteCmd::Open => Some(Action::OpenFile),
            PaletteCmd::Save => Some(Action::Save),
            PaletteCmd::SaveAs => Some(Action::SaveAs),
            PaletteCmd::CloseTab => Some(Action::CloseBuffer),
            PaletteCmd::Quit => Some(Action::Quit),
            PaletteCmd::Find => Some(Action::Find),
            PaletteCmd::FindReplace => Some(Action::FindReplace),
            PaletteCmd::FindInOpenTabs => Some(Action::FindInOpenTabs),
            PaletteCmd::AiAssistant => Some(Action::ToggleAiPanel),
            PaletteCmd::AiBrainstormIdeas => Some(Action::AiBrainstorm),
            PaletteCmd::GotoLine => Some(Action::GoToLine),
            PaletteCmd::GoToSymbol => Some(Action::GoToSymbol),
            PaletteCmd::GoToMatchingBracket => Some(Action::GoToMatchingBracket),
            PaletteCmd::ToggleFileTree => Some(Action::ToggleFileTree),
            PaletteCmd::NextTab => Some(Action::NextTab),
            PaletteCmd::PrevTab => Some(Action::PrevTab),
        }
    }
}

/// Palette UI state (owned by `App` in `app.rs`).
#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    pub visible: bool,
    pub filter: String,
    pub selected: usize,
    /// Indices into the filtered command list from the last [`rebuild_filtered`](crate::ui::command_palette::CommandPaletteState::rebuild_filtered) call.
    pub filtered_indices: Vec<usize>,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        Self {
            visible: false,
            filter: String::new(),
            selected: 0,
            filtered_indices: Vec::new(),
        }
    }

    pub fn open(&mut self) {
        self.visible = true;
        self.filter.clear();
        self.selected = 0;
        self.rebuild_filtered();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.selected = 0;
        self.filtered_indices.clear();
    }

    /// Recompute `filtered_indices` from `filter` (case-insensitive substring match on labels).
    pub fn rebuild_filtered(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered_indices.clear();
        let all = PaletteCmd::all();
        for (i, cmd) in all.iter().enumerate() {
            if q.is_empty() || Self::norm(cmd.label()).contains(&q) {
                self.filtered_indices.push(i);
            }
        }
        self.selected = self.selected.min(self.filtered_indices.len().saturating_sub(1));
    }

    fn norm(s: &str) -> String {
        s.to_lowercase()
    }

    pub fn selected_cmd(&self) -> Option<PaletteCmd> {
        let idx = self.filtered_indices.get(self.selected).copied()?;
        PaletteCmd::all().get(idx).copied()
    }
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self::new()
    }
}

/// Ratatui widget: palette overlay.
pub struct CommandPaletteWidget<'a> {
    pub state: &'a CommandPaletteState,
    pub theme: &'a Theme,
}

impl<'a> Widget for CommandPaletteWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.width < 20 || area.height < 5 || !self.state.visible {
            return;
        }

        let w = (area.width as usize).min(56).max(28) as u16;
        let h = (area.height / 2).min(14).max(6) as u16;
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;

        let border = Style::default()
            .fg(self.theme.ui.panel_border)
            .bg(self.theme.editor.background);
        let title = Style::default()
            .fg(self.theme.ui.status_bar_bg)
            .bg(self.theme.editor.background)
            .add_modifier(Modifier::BOLD);
        let normal = Style::default()
            .fg(self.theme.editor.foreground)
            .bg(self.theme.editor.background);
        let hi = Style::default()
            .fg(self.theme.editor.background)
            .bg(self.theme.ui.status_bar_bg);

        for row in 0..h {
            for col in 0..w {
                buf.set_string(x + col, y + row, " ", normal);
            }
        }

        buf.set_string(x + 1, y, "Command Palette", title);
        let filter_line = if self.state.filter.is_empty() {
            "> ".to_string()
        } else {
            format!("> {}", self.state.filter)
        };
        buf.set_string(x + 1, y + 1, &filter_line, normal);

        let all = PaletteCmd::all();
        let mut row = 2u16;
        let max_rows = h.saturating_sub(3);
        let mut shown = 0u16;
        for (list_i, &cmd_idx) in self.state.filtered_indices.iter().enumerate() {
            if shown >= max_rows {
                break;
            }
            let cmd = all[cmd_idx];
            let line = format!("  {}", cmd.label());
            let st = if list_i == self.state.selected {
                hi
            } else {
                normal
            };
            buf.set_string(x + 1, y + row, &line, st);
            row += 1;
            shown += 1;
        }

        buf.set_string(x + 1, y + h - 1, "↑↓ Enter Esc", border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_finds_save() {
        let mut s = CommandPaletteState::new();
        s.visible = true;
        s.filter = "save".into();
        s.rebuild_filtered();
        assert!(s
            .filtered_indices
            .iter()
            .any(|&i| PaletteCmd::all()[i] == PaletteCmd::Save));
    }
}
