/// Modal overlay system for find bar, go-to-line, and other dialogs.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::config::theme::Theme;
use crate::feature::search::SearchConfig;

/// Which control is focused on the find bar (Find or FindReplace find row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FindBarFocus {
    #[default]
    Query,
    ToggleCase,
    ToggleWord,
    ToggleRegex,
    Prev,
    Next,
    Close,
}

impl FindBarFocus {
    pub fn next(self) -> Self {
        match self {
            Self::Query => Self::ToggleCase,
            Self::ToggleCase => Self::ToggleWord,
            Self::ToggleWord => Self::ToggleRegex,
            Self::ToggleRegex => Self::Prev,
            Self::Prev => Self::Next,
            Self::Next => Self::Close,
            Self::Close => Self::Query,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Query => Self::Close,
            Self::ToggleCase => Self::Query,
            Self::ToggleWord => Self::ToggleCase,
            Self::ToggleRegex => Self::ToggleWord,
            Self::Prev => Self::ToggleRegex,
            Self::Next => Self::Prev,
            Self::Close => Self::Next,
        }
    }
}

/// Build [`SearchConfig`] from find UI state (find input + toggles).
pub fn search_config_from_modal(m: &ModalState) -> SearchConfig {
    SearchConfig {
        pattern: m.input.clone(),
        is_regex: m.use_regex,
        case_sensitive: m.match_case,
        whole_word: m.whole_word,
    }
}

/// Which field is focused in the find-replace dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FindReplaceFocus {
    #[default]
    Find,
    Replace,
}

/// Save As vs Open path prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPromptMode {
    SaveAs,
    Open,
}

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
    /// Enter file path (Save As or Open).
    PromptPath(PathPromptMode),
}

/// Modal state.
#[derive(Debug, Clone)]
pub struct ModalState {
    pub kind: ModalKind,
    pub input: String,
    pub replace_input: String,
    pub cursor_pos: usize,
    /// Cursor inside `replace_input` (find-replace only).
    pub replace_cursor_pos: usize,
    /// Active field in find-replace.
    pub find_replace_focus: FindReplaceFocus,
    pub message: Option<String>,
    /// Find bar: case-sensitive (Aa).
    pub match_case: bool,
    /// Find bar: whole word (ab).
    pub whole_word: bool,
    /// Find bar: regex (`.*`).
    pub use_regex: bool,
    /// Focus within the find row chrome (toggles, nav).
    pub find_bar_focus: FindBarFocus,
}

impl ModalState {
    fn default_find_options() -> (bool, bool, bool, FindBarFocus) {
        (false, false, false, FindBarFocus::Query)
    }

    /// Create a new find modal.
    pub fn find() -> Self {
        let (match_case, whole_word, use_regex, find_bar_focus) = Self::default_find_options();
        Self {
            kind: ModalKind::Find,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            replace_cursor_pos: 0,
            find_replace_focus: FindReplaceFocus::Find,
            message: None,
            match_case,
            whole_word,
            use_regex,
            find_bar_focus,
        }
    }

    /// Create a new find-replace modal.
    pub fn find_replace() -> Self {
        let (match_case, whole_word, use_regex, find_bar_focus) = Self::default_find_options();
        Self {
            kind: ModalKind::FindReplace,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            replace_cursor_pos: 0,
            find_replace_focus: FindReplaceFocus::Find,
            message: None,
            match_case,
            whole_word,
            use_regex,
            find_bar_focus,
        }
    }

    /// Create a go-to-line modal.
    pub fn goto_line() -> Self {
        let (match_case, whole_word, use_regex, find_bar_focus) = Self::default_find_options();
        Self {
            kind: ModalKind::GoToLine,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            replace_cursor_pos: 0,
            find_replace_focus: FindReplaceFocus::Find,
            message: None,
            match_case,
            whole_word,
            use_regex,
            find_bar_focus,
        }
    }

    /// Create a save confirmation modal.
    pub fn save_confirm(filename: &str) -> Self {
        let (match_case, whole_word, use_regex, find_bar_focus) = Self::default_find_options();
        Self {
            kind: ModalKind::SaveConfirm,
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            replace_cursor_pos: 0,
            find_replace_focus: FindReplaceFocus::Find,
            message: Some(format!(
                "Save changes to '{}'? (y) Save (n) Don't save (Esc) Cancel",
                filename
            )),
            match_case,
            whole_word,
            use_regex,
            find_bar_focus,
        }
    }

    /// Path prompt for Save As or Open.
    pub fn prompt_path(mode: PathPromptMode) -> Self {
        let (match_case, whole_word, use_regex, find_bar_focus) = Self::default_find_options();
        Self {
            kind: ModalKind::PromptPath(mode),
            input: String::new(),
            replace_input: String::new(),
            cursor_pos: 0,
            replace_cursor_pos: 0,
            find_replace_focus: FindReplaceFocus::Find,
            message: None,
            match_case,
            whole_word,
            use_regex,
            find_bar_focus,
        }
    }

    /// True when find query input should receive typing (find or find-replace find row).
    pub fn find_query_focused(&self) -> bool {
        let find_row = matches!(self.kind, ModalKind::Find)
            || (self.kind == ModalKind::FindReplace
                && self.find_replace_focus == FindReplaceFocus::Find);
        find_row && self.find_bar_focus == FindBarFocus::Query
    }

    pub fn toggle_find_replace_focus(&mut self) {
        self.find_replace_focus = match self.find_replace_focus {
            FindReplaceFocus::Find => FindReplaceFocus::Replace,
            FindReplaceFocus::Replace => FindReplaceFocus::Find,
        };
        if self.find_replace_focus == FindReplaceFocus::Find {
            self.find_bar_focus = FindBarFocus::Query;
        }
    }

    /// Insert a character into the primary path/find input, or replace field when focused.
    pub fn insert_char(&mut self, ch: char) {
        if self.kind == ModalKind::FindReplace && self.find_replace_focus == FindReplaceFocus::Replace
        {
            self.replace_input.insert(self.replace_cursor_pos, ch);
            self.replace_cursor_pos += 1;
        } else if self.find_query_focused() {
            self.input.insert(self.cursor_pos, ch);
            self.cursor_pos += 1;
        }
    }

    /// Delete the character before the cursor in the active field.
    pub fn backspace(&mut self) {
        if self.kind == ModalKind::FindReplace && self.find_replace_focus == FindReplaceFocus::Replace
        {
            if self.replace_cursor_pos > 0 {
                self.replace_cursor_pos -= 1;
                self.replace_input.remove(self.replace_cursor_pos);
            }
        } else if self.find_query_focused() && self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.input.remove(self.cursor_pos);
        }
    }

    /// Move cursor left in the active field.
    pub fn cursor_left(&mut self) {
        if self.kind == ModalKind::FindReplace && self.find_replace_focus == FindReplaceFocus::Replace
        {
            self.replace_cursor_pos = self.replace_cursor_pos.saturating_sub(1);
        } else if self.find_query_focused() && self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right in the active field.
    pub fn cursor_right(&mut self) {
        if self.kind == ModalKind::FindReplace && self.find_replace_focus == FindReplaceFocus::Replace
        {
            if self.replace_cursor_pos < self.replace_input.len() {
                self.replace_cursor_pos += 1;
            }
        } else if self.find_query_focused() && self.cursor_pos < self.input.len() {
            self.cursor_pos += 1;
        }
    }

    /// Get the input text.
    pub fn text(&self) -> &str {
        &self.input
    }
}

/// Tab on find-replace modal: cycle find-row controls or switch to the replace row.
pub fn find_replace_tab(modal: &mut ModalState) {
    if modal.find_replace_focus == FindReplaceFocus::Replace {
        modal.find_replace_focus = FindReplaceFocus::Find;
        modal.find_bar_focus = FindBarFocus::Query;
        return;
    }
    match modal.find_bar_focus {
        FindBarFocus::Query => {
            modal.find_replace_focus = FindReplaceFocus::Replace;
        }
        FindBarFocus::Close => {
            modal.find_replace_focus = FindReplaceFocus::Replace;
            modal.find_bar_focus = FindBarFocus::Query;
        }
        _ => {
            modal.find_bar_focus = modal.find_bar_focus.next();
        }
    }
}

/// Shift+Tab on find-replace modal.
pub fn find_replace_backtab(modal: &mut ModalState) {
    if modal.find_replace_focus == FindReplaceFocus::Replace {
        modal.find_replace_focus = FindReplaceFocus::Find;
        modal.find_bar_focus = FindBarFocus::Query;
        return;
    }
    match modal.find_bar_focus {
        FindBarFocus::Query => {
            modal.find_replace_focus = FindReplaceFocus::Replace;
        }
        _ => {
            modal.find_bar_focus = modal.find_bar_focus.prev();
        }
    }
}

/// Modal widget renderer.
pub struct ModalWidget<'a> {
    state: &'a ModalState,
    theme: &'a Theme,
    search_status: Option<String>,
    /// Frames remaining for focus pulse (0 = none).
    find_bar_anim: u8,
}

impl<'a> ModalWidget<'a> {
    /// Create a new modal widget.
    pub fn new(state: &'a ModalState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            search_status: None,
            find_bar_anim: 0,
        }
    }

    /// Set the search status text.
    pub fn search_status(mut self, status: Option<String>) -> Self {
        self.search_status = status;
        self
    }

    pub fn find_bar_anim(mut self, frames: u8) -> Self {
        self.find_bar_anim = frames;
        self
    }
}

fn draw_find_bar_row_fixed(
    buf: &mut RatBuffer,
    theme: &Theme,
    state: &ModalState,
    status: Option<&str>,
    bar_x: u16,
    bar_y: u16,
    bar_w: u16,
    anim: u8,
) {
    let bg = theme.ui.find_bar_bg;
    let fg = theme.editor.foreground;
    let base = Style::default().fg(fg).bg(bg);
    let pulse = anim > 0 && anim % 2 == 1;

    for x in bar_x..bar_x + bar_w {
        buf.set_string(x, bar_y, " ", base);
    }

    let mut x = bar_x;
    let label_style = Style::default()
        .fg(theme.ui.find_toggle_off_fg)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    buf.set_string(x, bar_y, " ", label_style);
    x += 1;

    let input_slot = bar_w.saturating_sub(28).max(8);
    let display: String = state.input.chars().take(input_slot as usize).collect();
    let input_style = Style::default().fg(fg).bg(theme.editor.background);
    let mut ix = x;
    for ch in display.chars() {
        if ix >= bar_x + bar_w {
            break;
        }
        buf.set_string(ix, bar_y, ch.to_string(), input_style);
        ix += 1;
    }
    while ix < x + input_slot && ix < bar_x + bar_w {
        buf.set_string(ix, bar_y, " ", input_style);
        ix += 1;
    }
    x += input_slot;

    let toggle_style = |on: bool, focused: bool| -> Style {
        let mut s = if on {
            Style::default()
                .fg(theme.editor.background)
                .bg(theme.ui.find_toggle_on_bg)
        } else {
            Style::default()
                .fg(theme.ui.find_toggle_off_fg)
                .bg(bg)
        };
        if focused {
            s = s.add_modifier(Modifier::BOLD);
            if pulse {
                s = s.add_modifier(Modifier::REVERSED);
            }
        }
        s
    };

    if x + 2 <= bar_x + bar_w {
        let f = state.find_bar_focus == FindBarFocus::ToggleCase;
        buf.set_string(
            x,
            bar_y,
            "Aa",
            toggle_style(state.match_case, f),
        );
        x += 2;
    }
    if x + 2 <= bar_x + bar_w {
        let f = state.find_bar_focus == FindBarFocus::ToggleWord;
        buf.set_string(
            x,
            bar_y,
            "ab",
            toggle_style(state.whole_word, f),
        );
        x += 2;
    }
    if x + 2 <= bar_x + bar_w {
        let f = state.find_bar_focus == FindBarFocus::ToggleRegex;
        buf.set_string(
            x,
            bar_y,
            ".*",
            toggle_style(state.use_regex, f),
        );
        x += 2;
    }

    if let Some(st) = status {
        let room = (bar_x + bar_w).saturating_sub(x).saturating_sub(6);
        if room > 1 {
            let take = room as usize;
            let s: String = st.chars().take(take).collect();
            let sty = Style::default()
                .fg(theme.ui.find_toggle_off_fg)
                .bg(bg);
            buf.set_string(x, bar_y, &s, sty);
            x += s.len() as u16;
        }
    }

    let nav_sty = |focus: FindBarFocus| {
        let f = state.find_bar_focus == focus;
        let mut s = Style::default().fg(fg).bg(bg);
        if f {
            s = s.add_modifier(Modifier::BOLD);
            if pulse {
                s = s.add_modifier(Modifier::REVERSED);
            }
        }
        s
    };
    if x + 1 <= bar_x + bar_w {
        buf.set_string(x, bar_y, "^", nav_sty(FindBarFocus::Prev));
        x += 1;
    }
    if x + 1 <= bar_x + bar_w {
        buf.set_string(x, bar_y, "v", nav_sty(FindBarFocus::Next));
        x += 1;
    }

    let close_f = state.find_bar_focus == FindBarFocus::Close;
    let mut csty = Style::default().fg(fg).bg(bg);
    if close_f {
        csty = csty.add_modifier(Modifier::BOLD);
        if pulse {
            csty = csty.add_modifier(Modifier::REVERSED);
        }
    }
    if x + 1 <= bar_x + bar_w {
        buf.set_string(x, bar_y, "x", csty);
    }

    if state.find_bar_focus == FindBarFocus::Query {
        let rel_c = state.cursor_pos.min(display.chars().count());
        let cx = bar_x + 1 + rel_c as u16;
        if cx < bar_x + 1 + input_slot {
            let cursor_style = Style::default()
                .bg(theme.editor.cursor)
                .fg(theme.editor.background);
            let ch = state
                .input
                .chars()
                .nth(state.cursor_pos)
                .map_or(' ', |c| c);
            buf.set_string(cx, bar_y, ch.to_string(), cursor_style);
        }
    }
}

impl<'a> Widget for ModalWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.height == 0 || area.width < 20 {
            return;
        }

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
            ModalKind::PromptPath(_) => 1,
        };

        match self.state.kind {
            ModalKind::Find => {
                let bar_w = area.width.min(72).max(28);
                let bar_x = area.x + area.width - bar_w;
                let bar_y = area.y;
                draw_find_bar_row_fixed(
                    buf,
                    self.theme,
                    self.state,
                    self.search_status.as_deref(),
                    bar_x,
                    bar_y,
                    bar_w,
                    self.find_bar_anim,
                );
            }
            ModalKind::FindReplace => {
                let bar_w = area.width.min(72).max(28);
                let bar_x = area.x + area.width - bar_w;
                let bar_y = area.y;
                if self.state.find_replace_focus == FindReplaceFocus::Find {
                    draw_find_bar_row_fixed(
                        buf,
                        self.theme,
                        self.state,
                        self.search_status.as_deref(),
                        bar_x,
                        bar_y,
                        bar_w,
                        self.find_bar_anim,
                    );
                } else {
                    for x in bar_x..bar_x + bar_w {
                        buf.set_string(x, bar_y, " ", input_style);
                    }
                    let label1 = "Find: ";
                    buf.set_string(bar_x, bar_y, label1, label_style);
                    buf.set_string(
                        bar_x + label1.len() as u16,
                        bar_y,
                        &self.state.input,
                        input_style,
                    );
                }

                let modal_width = area.width.min(80);
                let modal_x = area.x + (area.width - modal_width) / 2;
                for x in modal_x..modal_x + modal_width {
                    buf.set_string(x, bar_y + 1, " ", input_style);
                }
                let label2 = "Replace: ";
                buf.set_string(modal_x, bar_y + 1, label2, label_style);
                buf.set_string(
                    modal_x + label2.len() as u16,
                    bar_y + 1,
                    &self.state.replace_input,
                    input_style,
                );

                if self.state.find_replace_focus == FindReplaceFocus::Replace {
                    let cursor_style = Style::default()
                        .bg(self.theme.editor.cursor)
                        .fg(self.theme.editor.background);
                    let cx =
                        modal_x + label2.len() as u16 + self.state.replace_cursor_pos as u16;
                    if cx < modal_x + modal_width {
                        let ch = self
                            .state
                            .replace_input
                            .chars()
                            .nth(self.state.replace_cursor_pos)
                            .map_or(" ".to_string(), |c| c.to_string());
                        buf.set_string(cx, bar_y + 1, ch, cursor_style);
                    }
                }
            }
            ModalKind::GoToLine | ModalKind::SaveConfirm | ModalKind::PromptPath(_) => {
                let modal_width = area.width.min(80);
                let modal_x = area.x + (area.width - modal_width) / 2;
                let modal_y = area.y;
                for row in 0..height {
                    for x in modal_x..modal_x + modal_width {
                        buf.set_string(x, modal_y + row, " ", input_style);
                    }
                }
                match self.state.kind {
                    ModalKind::GoToLine => {
                        let label = "Go to Line: ";
                        buf.set_string(modal_x, modal_y, label, label_style);
                        buf.set_string(
                            modal_x + label.len() as u16,
                            modal_y,
                            &self.state.input,
                            input_style,
                        );
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
                    ModalKind::PromptPath(mode) => {
                        let label = match mode {
                            PathPromptMode::SaveAs => "Save As: ",
                            PathPromptMode::Open => "Open: ",
                        };
                        buf.set_string(modal_x, modal_y, label, label_style);
                        buf.set_string(
                            modal_x + label.len() as u16,
                            modal_y,
                            &self.state.input,
                            input_style,
                        );
                        let cursor_x = modal_x + label.len() as u16 + self.state.cursor_pos as u16;
                        if cursor_x < modal_x + modal_width {
                            let cursor_sy = Style::default()
                                .bg(self.theme.editor.cursor)
                                .fg(self.theme.editor.background);
                            let ch = self
                                .state
                                .input
                                .chars()
                                .nth(self.state.cursor_pos)
                                .map_or(" ".to_string(), |c| c.to_string());
                            buf.set_string(cursor_x, modal_y, ch, cursor_sy);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
