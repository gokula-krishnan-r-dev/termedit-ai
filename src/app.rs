/// Central application state and event loop.
///
/// The `App` struct holds all editor state and orchestrates the event loop:
/// terminal events → action mapping → state update → render.

use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(feature = "ai")]
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

#[cfg(feature = "ai")]
use crate::config::embed;
use crate::config::keymap::{self, Action};
use crate::config::settings::Settings;
use crate::config::theme::Theme;
use crate::core::cursor::SelectionMode;
use crate::core::document::Document;
#[cfg(feature = "ai")]
use crate::feature::ai_completion::{self, AiContext};
use crate::feature::completion;
#[cfg(feature = "ai")]
use crate::feature::gemini_chat::{
    self, resolve_chat_model_id, spawn_gemini_worker, ChatRole, GeminiChatRequest, GeminiTurn,
};
use crate::feature::search::{search_open_tabs, Search, SearchConfig};
use crate::feature::session::{self, SessionState};
use crate::feature::syntax::SyntaxHighlighter;
#[cfg(feature = "ai")]
use crate::ui::ai_panel::{AiPanelState, AiPanelWidget};
use crate::ui::command_palette::{CommandPaletteState, CommandPaletteWidget};
use crate::ui::editor_pane::EditorPane;
use crate::ui::open_tabs_palette::{OpenTabsPaletteState, OpenTabsPaletteWidget};
use crate::ui::outline_palette::{OutlinePaletteState, OutlinePaletteWidget};
use crate::ui::file_tree::FileTree;
use crate::ui::modal::{
    find_replace_backtab, find_replace_tab, search_config_from_modal, FindBarFocus, FindReplaceFocus,
    ModalKind, ModalState, ModalWidget, PathPromptMode,
};
use crate::ui::status_bar::StatusBar;
use crate::ui::tab_bar::{TabBar, TabInfo};

/// Pending action to run after the user answers the save-confirm modal.
#[derive(Debug, Clone)]
pub enum SaveConfirmPending {
    CloseTab(usize),
    Quit,
}

/// After Save As from a path prompt, optionally close a tab or finish quit-save-all.
#[derive(Debug, Clone)]
pub enum PathAfterSave {
    CloseTab(usize),
    QuitSaveAll,
}

/// Main application state.
pub struct App {
    /// Open documents (tabs).
    documents: Vec<Document>,
    /// Currently active tab index.
    active_tab: usize,
    /// Syntax highlighters per document.
    highlighters: Vec<SyntaxHighlighter>,
    /// Search state.
    search: Search,
    /// Active modal dialog.
    modal: Option<ModalState>,
    /// When SaveConfirm is shown, what to do after y/n.
    save_confirm_pending: Option<SaveConfirmPending>,
    /// After path prompt Save As succeeds, run this next.
    path_prompt_after_save: Option<PathAfterSave>,
    /// Ctrl+P command palette.
    command_palette: CommandPaletteState,
    /// Ctrl/Cmd+Shift+O symbol outline (empty when built without `outline` feature).
    outline_palette: OutlinePaletteState,
    /// Ctrl/Cmd+Shift+F search across open tabs.
    open_tabs_palette: OpenTabsPaletteState,
    /// Debounce deadline for Find in Open Tabs query scans.
    open_tabs_debounce_at: Option<Instant>,
    /// Editor settings.
    settings: Settings,
    /// Active theme.
    theme: Theme,
    /// Whether the file tree sidebar is visible.
    show_file_tree: bool,
    /// Whether the app should quit.
    should_quit: bool,
    /// Temporary status message.
    status_message: Option<String>,
    /// Dirty flag to avoid unnecessary re-renders.
    dirty: bool,
    /// Terminal viewport height (for page up/down and cursor visibility).
    viewport_height: usize,
    /// Terminal viewport width.
    viewport_width: usize,
    /// Inline completion dropdown: list of items, selected index, and prefix length for accept.
    completion_list: Option<CompletionList>,
    /// Find bar focus transition frames (non-zero = pulse in UI).
    find_bar_anim_frames: u8,
    /// Debounced regex search deadline.
    find_debounce_at: Option<Instant>,
    /// Pending regex [`SearchConfig`] after debounce.
    find_pending_config: Option<crate::feature::search::SearchConfig>,
    /// Pair of absolute char indices for bracket highlight (`()`, `[]`, `{}`), or None.
    bracket_highlight: Option<(usize, usize)>,
    #[cfg(feature = "ai")]
    /// Reused buffer when building AI context (reduces allocations).
    ai_context_before: String,
    #[cfg(feature = "ai")]
    /// Inline AI suggestion (ghost text) to show after cursor; Tab accepts.
    ghost_suggestion: Option<String>,
    #[cfg(feature = "ai")]
    /// Cursor (line, col) when suggestion was computed; clear suggestion if cursor moves line.
    ghost_trigger_pos: Option<(usize, usize)>,
    #[cfg(feature = "ai")]
    /// Channel to receive AI completion results (generation, suggestion).
    ai_rx: mpsc::Receiver<(u64, Option<String>)>,
    /// Kept so the AI worker's channel stays open (worker holds the paired receiver).
    #[cfg(feature = "ai")]
    #[allow(dead_code)]
    ai_tx: mpsc::Sender<(u64, Option<String>)>,
    #[cfg(feature = "ai")]
    /// Channel to send (generation, context) to the single AI worker.
    ai_request_tx: mpsc::Sender<(u64, AiContext)>,
    #[cfg(feature = "ai")]
    /// Generation ID to ignore stale AI responses.
    ai_generation: u64,
    #[cfg(feature = "ai")]
    /// When we last edited (for debounced AI request).
    last_ai_edit: Option<Instant>,
    #[cfg(feature = "ai")]
    /// Generation for which we already sent an AI request.
    ai_request_sent_for: Option<u64>,
    #[cfg(feature = "ai")]
    /// True when we sent an AI request for current generation and have not yet received a result.
    ai_pending: bool,
    #[cfg(feature = "ai")]
    /// Gemini chat overlay (Ctrl/Cmd+K).
    ai_panel: AiPanelState,
    #[cfg(feature = "ai")]
    gemini_rx: mpsc::Receiver<(u64, Result<String, gemini_chat::GeminiError>)>,
    #[cfg(feature = "ai")]
    gemini_generation: u64,
    #[cfg(feature = "ssh")]
    pub ssh_context: Option<crate::feature::ssh::SshContext>,
    #[cfg(feature = "ssh")]
    pub ssh_diff_state: Option<SshDiffState>,
}

#[cfg(feature = "ssh")]
pub struct SshDiffState {
    pub filename: String,
    pub diff: String,
    pub remote_path: std::path::PathBuf,
    pub scroll: u16,
}

/// State for the completion dropdown (keyword/buffer suggestions).
#[derive(Debug)]
pub struct CompletionList {
    pub items: Vec<String>,
    pub selected: usize,
    pub prefix_len: usize,
}

impl App {
    /// Create a new App with the given settings and theme.
    pub fn new(settings: Settings, theme: Theme) -> Self {
        let doc = Document::new();
        let hl = SyntaxHighlighter::new(&doc.language);
        #[cfg(feature = "ai")]
        let (ai_tx, ai_rx) = mpsc::channel();
        #[cfg(feature = "ai")]
        let (ai_request_tx, ai_request_rx) = mpsc::channel();
        #[cfg(feature = "ai")]
        let debounce_ms = settings.ai_debounce_ms;
        #[cfg(feature = "ai")]
        ai_completion::spawn_ai_worker(ai_request_rx, ai_tx.clone(), debounce_ms);

        #[cfg(feature = "ai")]
        let chat_model_id = resolve_chat_model_id(settings.ai_chat_model.as_deref());
        #[cfg(feature = "ai")]
        let (gemini_tx, gemini_rx) = mpsc::channel();
        #[cfg(feature = "ai")]
        let (gemini_request_tx, gemini_request_rx) = mpsc::channel();
        #[cfg(feature = "ai")]
        spawn_gemini_worker(gemini_request_rx, gemini_tx);

        Self {
            documents: vec![doc],
            active_tab: 0,
            highlighters: vec![hl],
            search: Search::new(),
            modal: None,
            save_confirm_pending: None,
            path_prompt_after_save: None,
            command_palette: CommandPaletteState::new(),
            outline_palette: OutlinePaletteState::new(),
            open_tabs_palette: OpenTabsPaletteState::new(),
            open_tabs_debounce_at: None,
            settings,
            theme,
            show_file_tree: false,
            should_quit: false,
            status_message: None,
            dirty: true,
            viewport_height: 24,
            viewport_width: 80,
            completion_list: None,
            find_bar_anim_frames: 0,
            find_debounce_at: None,
            find_pending_config: None,
            bracket_highlight: None,
            #[cfg(feature = "ai")]
            ai_context_before: String::new(),
            #[cfg(feature = "ai")]
            ghost_suggestion: None,
            #[cfg(feature = "ai")]
            ghost_trigger_pos: None,
            #[cfg(feature = "ai")]
            ai_rx,
            #[cfg(feature = "ai")]
            ai_tx,
            #[cfg(feature = "ai")]
            ai_request_tx,
            #[cfg(feature = "ai")]
            ai_generation: 0,
            #[cfg(feature = "ai")]
            last_ai_edit: None,
            #[cfg(feature = "ai")]
            ai_request_sent_for: None,
            #[cfg(feature = "ai")]
            ai_pending: false,
            #[cfg(feature = "ai")]
            ai_panel: AiPanelState::new(chat_model_id),
            #[cfg(feature = "ai")]
            gemini_rx,
            #[cfg(feature = "ai")]
            gemini_request_tx,
            #[cfg(feature = "ai")]
            gemini_generation: 0,
            #[cfg(feature = "ssh")]
            ssh_context: None,
            #[cfg(feature = "ssh")]
            ssh_diff_state: None,
        }
    }

    /// Clear inline ghost completion state (no-op when built without `ai`).
    fn clear_ai_inline_state(&mut self) {
        #[cfg(feature = "ai")]
        {
            self.ghost_suggestion = None;
            self.ghost_trigger_pos = None;
            self.ai_pending = false;
        }
    }

    fn close_ai_panel_overlay(&mut self) {
        #[cfg(feature = "ai")]
        self.ai_panel.close();
    }

    fn ai_panel_is_visible(&self) -> bool {
        #[cfg(feature = "ai")]
        {
            self.ai_panel.visible
        }
        #[cfg(not(feature = "ai"))]
        {
            false
        }
    }

    #[cfg(feature = "ai")]
    fn sync_ai_chat_model_setting(&mut self) {
        self.settings.ai_chat_model = Some(self.ai_panel.model_id.clone());
    }

    #[cfg(feature = "ai")]
    fn editor_width_cells(&self) -> u16 {
        let w = self.viewport_width as u16;
        if self.show_file_tree {
            w.saturating_sub(FileTree::width())
        } else {
            w
        }
    }

    #[cfg(feature = "ai")]
    fn resolve_gemini_api_key(&self) -> Option<String> {
        std::env::var("GEMINI_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.settings
                    .gemini_api_key
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .cloned()
            })
            .or_else(|| embed::embedded_gemini_api_key().map(|s| s.to_string()))
    }

    #[cfg(feature = "ai")]
    fn ai_panel_transcript_visible_rows(&self) -> usize {
        let editor_h = self.viewport_height as u16;
        let h = editor_h.saturating_sub(2).min(26).max(10);
        let footer_h = 2u16;
        h.saturating_sub(3 + footer_h) as usize
    }

    #[cfg(feature = "ai")]
    fn submit_ai_panel_message(&mut self) {
        let raw = std::mem::take(&mut self.ai_panel.input);
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        let text = trimmed.to_string();

        let Some(api_key) = self.resolve_gemini_api_key() else {
            self.ai_panel.error = Some(
                "Set GEMINI_API_KEY (env), gemini_api_key in config.toml, LOCAL_GEMINI_API_KEY in source, or build with TERMINEDIT_EMBEDDED_GEMINI_KEY.".to_string(),
            );
            self.dirty = true;
            return;
        };

        self.gemini_generation = self.gemini_generation.wrapping_add(1);
        let id = self.gemini_generation;
        self.ai_panel.pending_req_id = Some(id);
        self.ai_panel.loading = true;
        self.ai_panel.error = None;
        self.ai_panel.send_pulse = 3;
        self.ai_panel.stick_transcript_to_bottom = true;

        let tab = self.active_tab;
        let doc = &self.documents[tab];
        let file_label = doc.display_name();
        let lang = doc.language.clone();
        let system_instruction = gemini_chat::default_system_instruction(&file_label, &lang);

        self.ai_panel.turns.push(GeminiTurn {
            role: ChatRole::User,
            text,
        });

        let req = GeminiChatRequest {
            api_key,
            model_id: self.ai_panel.current_model_id().to_string(),
            system_instruction,
            turns: self.ai_panel.turns.clone(),
        };
        let _ = self.gemini_request_tx.send((id, req));
        self.dirty = true;
    }

    #[cfg(feature = "ai")]
    fn insert_last_ai_reply_at_cursor(&mut self) {
        let Some(text) = self.ai_panel.last_model_reply().map(str::to_string) else {
            self.status_message = Some("No AI reply to insert.".to_string());
            return;
        };
        let tab = self.active_tab;
        let doc = &mut self.documents[tab];
        if doc.cursor.selection.is_some() {
            let _ = doc.delete_selection();
        }
        doc.insert_text(&text);
        self.sync_bracket_highlight();
        self.dirty = true;
    }

    #[cfg(feature = "ai")]
    fn handle_ai_panel_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let super_ = key.modifiers.contains(KeyModifiers::SUPER);

        if matches!(key.code, KeyCode::Char('i')) && shift && (ctrl || super_) {
            self.insert_last_ai_reply_at_cursor();
            return;
        }
        if matches!(key.code, KeyCode::Char('k')) && (ctrl || super_) {
            self.sync_ai_chat_model_setting();
            self.close_ai_panel_overlay();
            self.dirty = true;
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.sync_ai_chat_model_setting();
                self.close_ai_panel_overlay();
                self.dirty = true;
            }
            KeyCode::Enter if !shift => {
                if !self.ai_panel.loading {
                    self.submit_ai_panel_message();
                }
            }
            KeyCode::Enter => {
                self.ai_panel.input.push('\n');
                self.dirty = true;
            }
            KeyCode::Tab => {
                self.ai_panel.cycle_model(1);
                self.sync_ai_chat_model_setting();
                self.dirty = true;
            }
            KeyCode::Char('m') if ctrl || super_ => {
                self.ai_panel.cycle_model(1);
                self.sync_ai_chat_model_setting();
                self.dirty = true;
            }
            KeyCode::PageUp => {
                let inner = AiPanelState::inner_width_from_editor(self.editor_width_cells());
                let vis = self.ai_panel_transcript_visible_rows().max(1);
                let lines = self.ai_panel.build_transcript_lines(inner);
                let max_scroll = lines.len().saturating_sub(vis);
                self.ai_panel.stick_transcript_to_bottom = false;
                self.ai_panel.transcript_scroll = self
                    .ai_panel
                    .transcript_scroll
                    .saturating_sub(vis)
                    .min(max_scroll);
                self.dirty = true;
            }
            KeyCode::PageDown => {
                let inner = AiPanelState::inner_width_from_editor(self.editor_width_cells());
                let vis = self.ai_panel_transcript_visible_rows().max(1);
                let lines = self.ai_panel.build_transcript_lines(inner);
                let max_scroll = lines.len().saturating_sub(vis);
                self.ai_panel.stick_transcript_to_bottom = false;
                self.ai_panel.transcript_scroll = (self.ai_panel.transcript_scroll + vis).min(max_scroll);
                if self.ai_panel.transcript_scroll >= max_scroll {
                    self.ai_panel.stick_transcript_to_bottom = true;
                }
                self.dirty = true;
            }
            KeyCode::Up => {
                let inner = AiPanelState::inner_width_from_editor(self.editor_width_cells());
                let vis = self.ai_panel_transcript_visible_rows().max(1);
                let lines = self.ai_panel.build_transcript_lines(inner);
                let max_scroll = lines.len().saturating_sub(vis);
                self.ai_panel.stick_transcript_to_bottom = false;
                self.ai_panel.transcript_scroll = self
                    .ai_panel
                    .transcript_scroll
                    .saturating_sub(1)
                    .min(max_scroll);
                self.dirty = true;
            }
            KeyCode::Down => {
                let inner = AiPanelState::inner_width_from_editor(self.editor_width_cells());
                let vis = self.ai_panel_transcript_visible_rows().max(1);
                let lines = self.ai_panel.build_transcript_lines(inner);
                let max_scroll = lines.len().saturating_sub(vis);
                self.ai_panel.stick_transcript_to_bottom = false;
                self.ai_panel.transcript_scroll = (self.ai_panel.transcript_scroll + 1).min(max_scroll);
                if self.ai_panel.transcript_scroll >= max_scroll {
                    self.ai_panel.stick_transcript_to_bottom = true;
                }
                self.dirty = true;
            }
            KeyCode::Backspace => {
                self.ai_panel.input.pop();
                self.dirty = true;
            }
            KeyCode::Char(c) if !ctrl && !super_ => {
                self.ai_panel.input.push(c);
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn sync_bracket_highlight(&mut self) {
        self.bracket_highlight = None;
        if !self.settings.bracket_matching {
            return;
        }
        let doc = &self.documents[self.active_tab];
        let max_c = self.settings.bracket_match_max_chars;
        if doc.buffer.len_chars() > max_c {
            return;
        }
        self.bracket_highlight = crate::feature::brackets::matching_bracket_pair_at_cursor(
            &doc.buffer,
            doc.cursor.line,
            doc.cursor.col,
            max_c,
        );
    }

    fn go_to_matching_bracket_action(&mut self) {
        let tab = self.active_tab;
        let max_c = self.settings.bracket_match_max_chars;
        let vh = self.viewport_height;
        let vw = self.viewport_width;

        let goto = {
            let doc = &self.documents[tab];
            if doc.buffer.len_chars() > max_c {
                self.status_message = Some(format!(
                    "Bracket match skipped: buffer longer than bracket_match_max_chars ({}).",
                    max_c
                ));
                None
            } else if let Some(bi) = crate::feature::brackets::resolve_bracket_index(
                &doc.buffer,
                doc.cursor.line,
                doc.cursor.col,
            ) {
                if let Some((lo, hi)) = crate::feature::brackets::matching_pair(&doc.buffer, bi) {
                    let target = if bi == lo { hi } else { lo };
                    let line = doc.buffer.char_to_line(target);
                    let line_start = doc.buffer.line_to_char(line);
                    let col = target - line_start;
                    Some((line, col))
                } else {
                    self.status_message = Some("No matching bracket.".to_string());
                    None
                }
            } else {
                self.status_message = Some("No bracket at cursor.".to_string());
                None
            }
        };

        if let Some((line, col)) = goto {
            let doc = &mut self.documents[tab];
            doc.cursor.goto(line, col, &doc.buffer);
            doc.ensure_cursor_visible(vh, vw);
        }

        self.clear_ai_inline_state();
        self.completion_list = None;
    }

    /// Build session state from current documents (for saving on exit).
    pub fn session_snapshot(&self) -> SessionState {
        session::snapshot(&self.documents, self.active_tab)
    }

    /// Apply restored session state (cursor and scroll per document). Call after opening files.
    pub fn restore_session(&mut self, state: &SessionState) {
        for (i, doc) in self.documents.iter_mut().enumerate() {
            if i >= state.states.len() {
                break;
            }
            let s = &state.states[i];
            let max_line = doc.buffer.line_count().saturating_sub(1);
            doc.cursor.line = s.line.min(max_line);
            let line_len = doc.buffer.line_len(doc.cursor.line);
            doc.cursor.col = s.col.min(line_len);
            doc.cursor.col_target = doc.cursor.col;
            doc.scroll_y = s.scroll_y;
            doc.scroll_x = s.scroll_x;
        }
        self.active_tab = state.active_tab.min(self.documents.len().saturating_sub(1));
        self.dirty = true;
    }

    /// Open files from a session state and restore cursor/scroll. Drops initial Untitled if present.
    pub fn restore_from_session(&mut self, state: &SessionState) {
        if state.paths.is_empty() {
            return;
        }
        for path_str in &state.paths {
            let path = Path::new(path_str);
            let _ = self.open_file(path);
        }
        if self.documents.len() > 1 && self.documents[0].buffer.file_path.is_none() {
            self.documents.remove(0);
            self.highlighters.remove(0);
            if self.active_tab > 0 {
                self.active_tab -= 1;
            } else {
                self.active_tab = 0;
            }
        }
        if self.documents.len() == state.paths.len() {
            self.restore_session(state);
        }
    }

    /// Open a file and add it as a new tab.
    /// Remove the initial empty document when at least one CLI file tab exists.
    pub fn drop_cli_placeholder_tab_if_redundant(&mut self) {
        if self.documents.len() <= 1 {
            return;
        }
        let d = &self.documents[0];
        if d.buffer.file_path.is_none()
            && !d.is_modified()
            && d.buffer.rope.len_chars() == 0
        {
            self.remove_tab_at(0);
        }
    }

    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        // Check if already open
        for (i, doc) in self.documents.iter().enumerate() {
            if doc.buffer.file_path.as_deref() == Some(path) {
                self.active_tab = i;
                self.dirty = true;
                return Ok(());
            }
        }

        let doc = Document::open(path)?;
        let hl = SyntaxHighlighter::new(&doc.language);
        self.documents.push(doc);
        self.highlighters.push(hl);
        self.active_tab = self.documents.len() - 1;
        self.dirty = true;
        Ok(())
    }

    fn refresh_tab_language(&mut self, tab: usize) {
        if tab >= self.documents.len() {
            return;
        }
        self.documents[tab].refresh_language();
        self.highlighters[tab] = SyntaxHighlighter::new(&self.documents[tab].language);
    }

    fn remove_tab_at(&mut self, t: usize) {
        if t >= self.documents.len() {
            return;
        }
        self.documents.remove(t);
        self.highlighters.remove(t);
        if self.active_tab > t {
            self.active_tab -= 1;
        } else if self.active_tab == t {
            self.active_tab = t.min(self.documents.len().saturating_sub(1));
        }
        if self.documents.is_empty() {
            self.should_quit = true;
        }
    }

    fn save_modified_with_paths(&mut self) -> Result<(), String> {
        for i in 0..self.documents.len() {
            if self.documents[i].is_modified() && self.documents[i].buffer.file_path.is_some() {
                self.documents[i].save().map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn after_path_prompt_success(&mut self) {
        match self.path_prompt_after_save.take() {
            Some(PathAfterSave::CloseTab(t)) => {
                self.remove_tab_at(t);
            }
            Some(PathAfterSave::QuitSaveAll) => {
                if let Err(e) = self.save_modified_with_paths() {
                    self.status_message = Some(e);
                    self.path_prompt_after_save = Some(PathAfterSave::QuitSaveAll);
                    return;
                }
                if let Some(i) = self.documents.iter().position(|d| d.is_modified()) {
                    self.active_tab = i;
                    self.path_prompt_after_save = Some(PathAfterSave::QuitSaveAll);
                    self.modal = Some(ModalState::prompt_path(PathPromptMode::SaveAs));
                } else {
                    self.should_quit = true;
                }
            }
            None => {}
        }
    }

    /// Expand leading `~` to the user home directory.
    fn expand_user_path(path_str: &str) -> PathBuf {
        let trimmed = path_str.trim();
        if let Some(rest) = trimmed.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        } else if trimmed == "~" {
            if let Some(home) = dirs::home_dir() {
                return home;
            }
        }
        PathBuf::from(trimmed)
    }

    fn submit_path_prompt(&mut self, mode: PathPromptMode, path_str: &str) {
        let trimmed = path_str.trim();
        if trimmed.is_empty() {
            self.status_message = Some("Path is empty".into());
            return;
        }
        let mut path = Self::expand_user_path(trimmed);

        #[cfg(feature = "ssh")]
        if self.ssh_context.is_some() && mode == PathPromptMode::Open {
            let ctx = self.ssh_context.as_mut().unwrap();
            let remote_path = path.clone();
            match ctx.rt.block_on(ctx.sync.download_file(&remote_path)) {
                Ok(local_path) => {
                    ctx.local_to_remote.insert(local_path.clone(), remote_path.clone());
                    path = local_path;
                }
                Err(e) => {
                    self.status_message = Some(format!("Failed to download {}: {}", remote_path.display(), e));
                    return;
                }
            }
        }

        let tab = self.active_tab;
        match mode {
            PathPromptMode::SaveAs => match self.documents[tab].save_as(&path) {
                Ok(()) => {
                    self.refresh_tab_language(tab);
                    self.status_message = Some("Saved".into());
                    self.modal = None;
                    self.after_path_prompt_success();
                }
                Err(e) => self.status_message = Some(format!("Save failed: {}", e)),
            },
            PathPromptMode::Open => {
                self.modal = None;
                match self.open_file(&path) {
                    Ok(()) => self.status_message = Some("Opened".into()),
                    Err(e) => self.status_message = Some(format!("Open failed: {}", e)),
                }
            }
        }
    }

    fn run_palette_selection(&mut self) {
        let Some(cmd) = self.command_palette.selected_cmd() else {
            return;
        };
        self.command_palette.close();
        if let Some(action) = cmd.to_action() {
            self.handle_action(action);
        }
    }

    fn handle_command_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.command_palette.close(),
            KeyCode::Enter => self.run_palette_selection(),
            KeyCode::Up => {
                self.command_palette.selected = self.command_palette.selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self
                    .command_palette
                    .filtered_indices
                    .len()
                    .saturating_sub(1);
                self.command_palette.selected = (self.command_palette.selected + 1).min(max);
            }
            KeyCode::Backspace => {
                self.command_palette.filter.pop();
                self.command_palette.rebuild_filtered();
            }
            KeyCode::Char(c) => {
                self.command_palette.filter.push(c);
                self.command_palette.rebuild_filtered();
            }
            _ => {}
        }
    }

    fn open_find_in_open_tabs_palette(&mut self) {
        self.command_palette.close();
        self.outline_palette.close();
        self.close_ai_panel_overlay();
        self.open_tabs_palette.open();
        self.open_tabs_debounce_at = None;
    }

    fn schedule_open_tabs_search_debounce(&mut self) {
        let ms = self.settings.find_in_open_tabs_debounce_ms.max(1);
        self.open_tabs_debounce_at = Some(Instant::now() + Duration::from_millis(ms));
    }

    fn refresh_open_tabs_search_results(&mut self) {
        let q = self.open_tabs_palette.query.clone();
        if q.is_empty() {
            self.open_tabs_palette.hits.clear();
            self.open_tabs_palette.last_error = None;
            self.open_tabs_palette.hint = None;
            self.open_tabs_palette.selected = 0;
            self.open_tabs_palette.scroll = 0;
            return;
        }
        let config = SearchConfig {
            pattern: q,
            is_regex: self.settings.find_in_open_tabs_regex,
            case_sensitive: self.settings.find_in_open_tabs_case_sensitive,
            whole_word: self.settings.find_in_open_tabs_whole_word,
        };
        let max_r = self.settings.find_in_open_tabs_max_results;
        let max_c = self.settings.find_in_open_tabs_max_chars_per_tab;
        let (hits, skipped, err) = search_open_tabs(&self.documents, &config, max_r, max_c);
        self.open_tabs_palette.hits = hits;
        let have_err = err.is_some();
        self.open_tabs_palette.last_error = err;
        self.open_tabs_palette.hint = if have_err {
            if skipped > 0 {
                Some(format!(
                    "Also: {} tab(s) skipped (larger than find_in_open_tabs_max_chars_per_tab).",
                    skipped
                ))
            } else {
                None
            }
        } else if skipped > 0 {
            Some(format!(
                "Skipped {} tab(s) larger than find_in_open_tabs_max_chars_per_tab ({}).",
                skipped, max_c
            ))
        } else if self.open_tabs_palette.hits.is_empty() {
            Some("No matches in open tabs.".to_string())
        } else {
            None
        };
        if !self.open_tabs_palette.hits.is_empty() {
            self.open_tabs_palette.selected = self
                .open_tabs_palette
                .selected
                .min(self.open_tabs_palette.hits.len().saturating_sub(1));
        } else {
            self.open_tabs_palette.selected = 0;
        }
        self.open_tabs_palette.scroll = 0;
    }

    fn flush_open_tabs_debounce_if_ready(&mut self) -> bool {
        if !self.open_tabs_palette.visible {
            return false;
        }
        let Some(deadline) = self.open_tabs_debounce_at else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }
        self.open_tabs_debounce_at = None;
        self.refresh_open_tabs_search_results();
        true
    }

    fn run_open_tabs_selection(&mut self) {
        let hit = self.open_tabs_palette.selected_hit().cloned();
        let Some(hit) = hit else {
            return;
        };
        let vh = self.viewport_height;
        let vw = self.viewport_width;
        self.open_tabs_palette.close();
        self.open_tabs_debounce_at = None;
        self.active_tab = hit.tab_index;
        self.clear_ai_inline_state();
        self.completion_list = None;
        let doc = &mut self.documents[self.active_tab];
        let line = doc.buffer.char_to_line(hit.match_start);
        let line_start = doc.buffer.line_to_char(line);
        let col = hit.match_start.saturating_sub(line_start);
        doc.cursor.goto(line, col, &doc.buffer);
        doc.cursor.clear_selection();
        doc.ensure_cursor_visible(vh, vw);
    }

    fn handle_open_tabs_palette_key(&mut self, key: KeyEvent) {
        let vis = Self::OUTLINE_LIST_VISIBLE;
        match key.code {
            KeyCode::Esc => {
                self.open_tabs_palette.close();
                self.open_tabs_debounce_at = None;
            }
            KeyCode::Enter => self.run_open_tabs_selection(),
            KeyCode::Up => {
                self.open_tabs_palette.move_selection(-1, vis);
            }
            KeyCode::Down => {
                self.open_tabs_palette.move_selection(1, vis);
            }
            KeyCode::Backspace => {
                self.open_tabs_palette.query.pop();
                self.schedule_open_tabs_search_debounce();
            }
            KeyCode::Char(c) => {
                self.open_tabs_palette.query.push(c);
                self.schedule_open_tabs_search_debounce();
            }
            _ => {}
        }
    }

    const OUTLINE_LIST_VISIBLE: usize = 10;

    fn open_outline_palette(&mut self) {
        self.command_palette.close();
        self.open_tabs_palette.close();
        self.close_ai_panel_overlay();
        self.open_tabs_debounce_at = None;
        let tab = self.active_tab;
        let doc = &self.documents[tab];
        let max_b = self.settings.outline_max_bytes;
        let nbytes = doc.buffer.rope.len_bytes();
        if nbytes > max_b {
            self.outline_palette.open_into(
                vec![],
                Some(format!(
                    "File too large for outline ({} bytes; outline_max_bytes = {}).",
                    nbytes, max_b
                )),
                None,
            );
            return;
        }
        let text = doc.buffer.to_string();
        let syms = crate::feature::outline::extract_symbols(&doc.language, &text);
        let hint = if syms.is_empty() {
            Some("No symbols for this language.".to_string())
        } else {
            None
        };
        self.outline_palette.open_into(syms, hint, None);
    }

    fn run_outline_selection(&mut self) {
        let sym = self.outline_palette.selected_symbol().cloned();
        let vh = self.viewport_height;
        let vw = self.viewport_width;
        self.outline_palette.close();
        let Some(sym) = sym else {
            return;
        };
        let tab = self.active_tab;
        self.clear_ai_inline_state();
        self.completion_list = None;
        let doc = &mut self.documents[tab];
        doc.cursor
            .goto(sym.start_line, sym.name_start_col, &doc.buffer);
        doc.ensure_cursor_visible(vh, vw);
    }

    fn handle_outline_palette_key(&mut self, key: KeyEvent) {
        let vis = Self::OUTLINE_LIST_VISIBLE;
        match key.code {
            KeyCode::Esc => self.outline_palette.close(),
            KeyCode::Enter => self.run_outline_selection(),
            KeyCode::Up => {
                self.outline_palette.move_selection(-1, vis);
            }
            KeyCode::Down => {
                self.outline_palette.move_selection(1, vis);
            }
            KeyCode::Backspace => {
                self.outline_palette.filter.pop();
                self.outline_palette.rebuild_filtered();
            }
            KeyCode::Char(c) => {
                self.outline_palette.filter.push(c);
                self.outline_palette.rebuild_filtered();
            }
            _ => {}
        }
    }

    /// Run the main application event loop.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let result = self.event_loop(&mut terminal);

        if self.settings.session_restore {
            if let Some(path) = session::default_session_path() {
                let state = self.session_snapshot();
                state.save_to(&path);
            }
        }

        disable_raw_mode()?;
        terminal.backend_mut().execute(LeaveAlternateScreen)?;
        terminal.backend_mut().execute(DisableMouseCapture)?;
        terminal.show_cursor()?;

        // Ensure shell prompt appears on a new line after exit
        let _ = io::stdout().write_all(b"\r\n");
        let _ = io::stdout().flush();

        result
    }

    /// The core event loop.
    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        self.sync_bracket_highlight();
        loop {
            if self.dirty {
                terminal.draw(|frame| self.render(frame))?;
                self.dirty = false;
            }

            #[cfg(feature = "ai")]
            {
                if self.settings.ai_enabled {
                    while let Ok((gen, suggestion)) = self.ai_rx.try_recv() {
                        if gen == self.ai_generation {
                            self.ghost_suggestion = suggestion;
                            self.ai_pending = false;
                            self.dirty = true;
                        }
                    }
                    if let Some(last) = self.last_ai_edit {
                        if last.elapsed() >= Duration::from_millis(self.settings.ai_debounce_ms)
                            && self.ai_request_sent_for != Some(self.ai_generation)
                            && !self.documents.is_empty()
                        {
                            let tab = self.active_tab;
                            let doc = &self.documents[tab];
                            let line = doc.buffer.line_text(doc.cursor.line);
                            let line_prefix: String = line.chars().take(doc.cursor.col).collect();
                            let start_line = doc.cursor.line.saturating_sub(30);
                            self.ai_context_before.clear();
                            for j in start_line..doc.cursor.line {
                                if j > start_line {
                                    self.ai_context_before.push('\n');
                                }
                                self.ai_context_before
                                    .push_str(&doc.buffer.line_text(j));
                            }
                            let context = AiContext {
                                line_prefix,
                                context_before: self.ai_context_before.clone(),
                                language: doc.language.clone(),
                                path: doc.buffer.file_path.as_ref().and_then(|p| p.to_str()).map(String::from),
                                model: self.settings.ai_model.clone(),
                            };
                            let _ = self.ai_request_tx.send((self.ai_generation, context));
                            self.ai_request_sent_for = Some(self.ai_generation);
                            self.ai_pending = true;
                            self.dirty = true;
                        }
                    }
                }

                while let Ok((gen, reply)) = self.gemini_rx.try_recv() {
                    if self.ai_panel.pending_req_id == Some(gen) {
                        self.ai_panel.pending_req_id = None;
                        self.ai_panel.loading = false;
                        match reply {
                            Ok(text) => {
                                self.ai_panel.turns.push(GeminiTurn {
                                    role: ChatRole::Model,
                                    text,
                                });
                                self.ai_panel.error = None;
                                self.ai_panel.stick_transcript_to_bottom = true;
                            }
                            Err(e) => {
                                self.ai_panel.error = Some(e.to_string());
                                if let Some(t) = self.ai_panel.turns.pop() {
                                    if matches!(t.role, ChatRole::User) {
                                        self.ai_panel.input = t.text;
                                    } else {
                                        self.ai_panel.turns.push(t);
                                    }
                                }
                            }
                        }
                        self.dirty = true;
                    }
                }
            }

            if event::poll(Duration::from_millis(16))? {
                let evt = event::read()?;
                self.handle_event(evt);
                self.dirty = true;

                if self.should_quit {
                    break;
                }
            } else {
                let mut tick_dirty = false;
                if self.tick_find_bar_animation() {
                    tick_dirty = true;
                }
                if self.flush_find_debounce_if_ready() {
                    tick_dirty = true;
                }
                if self.flush_open_tabs_debounce_if_ready() {
                    tick_dirty = true;
                }
                #[cfg(feature = "ai")]
                if self.ai_panel.visible
                    && (self.ai_panel.loading || self.ai_panel.send_pulse > 0)
                {
                    self.ai_panel.tick_spinner();
                    tick_dirty = true;
                }
                if tick_dirty {
                    self.dirty = true;
                }
            }
        }
        Ok(())
    }

    /// Handle a terminal event.
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key_event) => {
                if self.open_tabs_palette.visible {
                    self.handle_open_tabs_palette_key(key_event);
                } else if self.outline_palette.visible {
                    self.handle_outline_palette_key(key_event);
                } else if self.command_palette.visible {
                    self.handle_command_palette_key(key_event);
                } else if self.ai_panel_is_visible() {
                    #[cfg(feature = "ai")]
                    self.handle_ai_panel_key(key_event);
                } else if {
                    #[cfg(feature = "ssh")]
                    { self.ssh_diff_state.is_some() }
                    #[cfg(not(feature = "ssh"))]
                    { false }
                } {
                    #[cfg(feature = "ssh")]
                    self.handle_ssh_diff_key(key_event);
                } else if self.modal.is_some() {
                    self.handle_modal_key(key_event);
                } else if let Some(ref mut comp) = self.completion_list {
                    let action = keymap::map_key_event(key_event);
                    match action {
                        Action::MoveDown => {
                            comp.selected = (comp.selected + 1).min(comp.items.len().saturating_sub(1));
                            self.dirty = true;
                        }
                        Action::MoveUp => {
                            comp.selected = comp.selected.saturating_sub(1);
                            self.dirty = true;
                        }
                        Action::InsertTab | Action::InsertNewline => {
                            if comp.selected < comp.items.len() {
                                let replacement = comp.items[comp.selected].clone();
                                let prefix_len = comp.prefix_len;
                                self.completion_list = None;
                                self.clear_ai_inline_state();
                                self.documents[self.active_tab].replace_before_cursor(prefix_len, &replacement);
                                self.dirty = true;
                            } else {
                                self.handle_action(action);
                            }
                        }
                        Action::EscapeSearch => {
                            self.completion_list = None;
                            self.dirty = true;
                        }
                        _ => {
                            self.handle_action(action);
                        }
                    }
                } else {
                    let action = keymap::map_key_event(key_event);
                    self.handle_action(action);
                }
            }
            Event::Mouse(mouse_event) => {
                self.handle_mouse(mouse_event);
            }
            Event::Resize(w, h) => {
                self.viewport_width = w as usize;
                self.viewport_height = h.saturating_sub(2) as usize;
                self.dirty = true;
            }
            _ => {}
        }
    }

    #[cfg(feature = "ssh")]
    fn handle_ssh_diff_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.ssh_diff_state = None;
                self.status_message = Some("Sync cancelled.".to_string());
                self.dirty = true;
            }
            KeyCode::Char('y') => {
                if let Some(state) = self.ssh_diff_state.take() {
                    if let Some(ctx) = &mut self.ssh_context {
                        let tab = self.active_tab;
                        let local_path = self.documents[tab].buffer.file_path.as_ref().unwrap().clone();
                        match self.documents[tab].save() {
                            Ok(()) => {
                                match ctx.rt.block_on(ctx.sync.upload_file(&local_path, &state.remote_path)) {
                                    Ok(()) => self.status_message = Some("Sync successful.".to_string()),
                                    Err(e) => self.status_message = Some(format!("Sync failed: {}", e)),
                                }
                            }
                            Err(e) => self.status_message = Some(format!("Local save failed: {}", e)),
                        }
                    }
                }
                self.dirty = true;
            }
            KeyCode::Up => {
                if let Some(state) = &mut self.ssh_diff_state {
                    state.scroll = state.scroll.saturating_sub(1);
                    self.dirty = true;
                }
            }
            KeyCode::Down => {
                if let Some(state) = &mut self.ssh_diff_state {
                    state.scroll = state.scroll.saturating_add(1);
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// Handle a keyed action.
    ///
    /// Uses direct indexing into `self.documents[self.active_tab]` to avoid
    /// borrow checker issues with the `doc()` / `doc_mut()` helper pattern.
    fn handle_action(&mut self, action: Action) {
        self.status_message = None;
        let tab = self.active_tab;
        let tab_size = self.settings.tab_size;
        let soft_tabs = self.settings.soft_tabs;
        let vh = self.viewport_height;
        let vw = self.viewport_width;

        match action {
            // === Movement ===
            Action::MoveLeft => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_left(&doc.buffer);
            }
            Action::MoveRight => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_right(&doc.buffer);
            }
            Action::MoveUp => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_up(&doc.buffer);
            }
            Action::MoveDown => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_down(&doc.buffer);
            }
            Action::WordLeft => {
                let doc = &mut self.documents[tab];
                doc.cursor.word_left(&doc.buffer);
            }
            Action::WordRight => {
                let doc = &mut self.documents[tab];
                doc.cursor.word_right(&doc.buffer);
            }
            Action::Home => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_home(&doc.buffer);
            }
            Action::End => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_end(&doc.buffer);
            }
            Action::FileStart => {
                self.documents[tab].cursor.move_file_start();
            }
            Action::FileEnd => {
                let doc = &mut self.documents[tab];
                doc.cursor.move_file_end(&doc.buffer);
            }
            Action::PageUp => {
                let doc = &mut self.documents[tab];
                doc.cursor.page_up(vh, &doc.buffer);
            }
            Action::PageDown => {
                let doc = &mut self.documents[tab];
                doc.cursor.page_down(vh, &doc.buffer);
            }

            // === Selection ===
            Action::SelectLeft => {
                let doc = &mut self.documents[tab];
                doc.cursor.select_left(&doc.buffer);
            }
            Action::SelectRight => {
                let doc = &mut self.documents[tab];
                doc.cursor.select_right(&doc.buffer);
            }
            Action::SelectUp => {
                let doc = &mut self.documents[tab];
                doc.cursor.select_up(&doc.buffer);
            }
            Action::SelectDown => {
                let doc = &mut self.documents[tab];
                doc.cursor.select_down(&doc.buffer);
            }
            Action::SelectAll => {
                let doc = &mut self.documents[tab];
                doc.cursor.select_all(&doc.buffer);
            }
            Action::SelectLine => {
                let doc = &mut self.documents[tab];
                doc.cursor.select_line(&doc.buffer);
            }
            Action::SelectWordLeft => {
                let doc = &mut self.documents[tab];
                doc.cursor.start_selection(SelectionMode::Char);
                doc.cursor.word_left(&doc.buffer);
            }
            Action::SelectWordRight => {
                let doc = &mut self.documents[tab];
                doc.cursor.start_selection(SelectionMode::Char);
                doc.cursor.word_right(&doc.buffer);
            }
            Action::SelectHome => {
                let doc = &mut self.documents[tab];
                doc.cursor.start_selection(SelectionMode::Char);
                doc.cursor.col = 0;
                doc.cursor.col_target = 0;
            }
            Action::SelectEnd => {
                let doc = &mut self.documents[tab];
                doc.cursor.start_selection(SelectionMode::Char);
                let len = doc.buffer.line_len(doc.cursor.line);
                doc.cursor.col = len;
                doc.cursor.col_target = len;
            }

            // === Editing ===
            Action::InsertChar(ch) => {
                let doc = &mut self.documents[tab];
                if doc.cursor.has_selection() {
                    doc.delete_selection();
                }
                doc.insert_char(ch);
                if self.settings.ai_enabled {
                    #[cfg(feature = "ai")]
                    {
                        self.ghost_trigger_pos = Some((doc.cursor.line, doc.cursor.col));
                        self.last_ai_edit = Some(Instant::now());
                        self.ai_generation = self.ai_generation.wrapping_add(1);
                        self.ghost_suggestion = completion::suggest(doc);
                    }
                }
                if let Some((items, prefix_len)) = completion::suggest_list(doc) {
                    self.completion_list = Some(CompletionList { items, selected: 0, prefix_len });
                } else {
                    self.completion_list = None;
                }
            }
            Action::InsertNewline => {
                let doc = &mut self.documents[tab];
                if doc.cursor.has_selection() {
                    doc.delete_selection();
                }
                doc.insert_char('\n');
            }
            Action::InsertTab => {
                #[cfg(feature = "ai")]
                let consumed_ghost = if let Some(text) = self.ghost_suggestion.take() {
                    self.ghost_trigger_pos = None;
                    self.documents[tab].insert_text(&text);
                    true
                } else {
                    false
                };
                #[cfg(not(feature = "ai"))]
                let consumed_ghost = false;
                if !consumed_ghost {
                    let doc = &mut self.documents[tab];
                    if doc.cursor.has_selection() {
                        doc.indent(tab_size);
                    } else if soft_tabs {
                        let spaces = " ".repeat(tab_size);
                        doc.insert_text(&spaces);
                    } else {
                        doc.insert_char('\t');
                    }
                }
            }
            Action::Backspace => {
                let doc = &mut self.documents[tab];
                if doc.cursor.has_selection() {
                    doc.delete_selection();
                } else {
                    doc.backspace();
                }
                if self.settings.ai_enabled {
                    #[cfg(feature = "ai")]
                    {
                        self.ghost_trigger_pos = Some((doc.cursor.line, doc.cursor.col));
                        self.last_ai_edit = Some(Instant::now());
                        self.ai_generation = self.ai_generation.wrapping_add(1);
                        self.ghost_suggestion = completion::suggest(doc);
                    }
                }
                if let Some((items, prefix_len)) = completion::suggest_list(doc) {
                    self.completion_list = Some(CompletionList { items, selected: 0, prefix_len });
                } else {
                    self.completion_list = None;
                }
            }
            Action::Delete => {
                let doc = &mut self.documents[tab];
                if doc.cursor.has_selection() {
                    doc.delete_selection();
                } else {
                    doc.delete_char();
                }
                if self.settings.ai_enabled {
                    #[cfg(feature = "ai")]
                    {
                        self.ghost_trigger_pos = Some((doc.cursor.line, doc.cursor.col));
                        self.last_ai_edit = Some(Instant::now());
                        self.ai_generation = self.ai_generation.wrapping_add(1);
                        self.ghost_suggestion = completion::suggest(doc);
                    }
                }
                if let Some((items, prefix_len)) = completion::suggest_list(doc) {
                    self.completion_list = Some(CompletionList { items, selected: 0, prefix_len });
                } else {
                    self.completion_list = None;
                }
            }
            Action::Undo => {
                self.documents[tab].undo();
            }
            Action::Redo => {
                self.documents[tab].redo();
            }
            Action::DeleteLine => {
                self.documents[tab].delete_line();
            }
            Action::MoveLineUp => {
                self.documents[tab].move_line_up();
            }
            Action::MoveLineDown => {
                self.documents[tab].move_line_down();
            }
            Action::ToggleComment => {
                self.documents[tab].toggle_comment();
            }
            Action::Indent => {
                self.documents[tab].indent(tab_size);
            }
            Action::Dedent => {
                self.documents[tab].dedent(tab_size);
            }
            Action::DuplicateLine => {
                self.documents[tab].duplicate_line();
            }

            // === Clipboard ===
            Action::Copy => {
                #[cfg(feature = "clipboard")]
                {
                    let doc = &self.documents[tab];
                    if let Some(text) = doc.cursor.selected_text(&doc.buffer) {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(text);
                            self.status_message = Some("Copied".to_string());
                        }
                    }
                }
                #[cfg(not(feature = "clipboard"))]
                {
                    self.status_message = Some("Clipboard unsupported in this build.".to_string());
                }
            }
            Action::Cut => {
                #[cfg(feature = "clipboard")]
                {
                    let doc = &self.documents[tab];
                    if let Some(text) = doc.cursor.selected_text(&doc.buffer) {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(text);
                        }
                    }
                }
                self.documents[tab].delete_selection();
                self.status_message = Some("Cut".to_string());
            }
            Action::Paste => {
                #[cfg(feature = "clipboard")]
                {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            let doc = &mut self.documents[tab];
                            if doc.cursor.has_selection() {
                                doc.delete_selection();
                            }
                            doc.insert_text(&text);
                        }
                    }
                }
                #[cfg(not(feature = "clipboard"))]
                {
                    self.status_message = Some("Clipboard unsupported in this build.".to_string());
                }
            }

            // === File Operations ===
            Action::Save => {
                if self.documents[tab].buffer.file_path.is_none() {
                    self.path_prompt_after_save = None;
                    self.modal = Some(ModalState::prompt_path(PathPromptMode::SaveAs));
                } else {
                    let mut is_remote = false;
                    #[cfg(feature = "ssh")]
                    if let Some(ctx) = &self.ssh_context {
                        let local_path = self.documents[tab].buffer.file_path.as_ref().unwrap();
                        if let Some(remote_path) = ctx.local_to_remote.get(local_path) {
                            is_remote = true;
                            let modified = self.documents[tab].buffer.to_string();
                            let original = ctx.rt.block_on(ctx.sync.get_remote_content(remote_path)).unwrap_or_default();
                            let diff = crate::feature::ssh::sync::SshSyncManager::compute_diff(&original, &modified);
                            self.ssh_diff_state = Some(SshDiffState {
                                filename: remote_path.to_string_lossy().into_owned(),
                                diff,
                                remote_path: remote_path.to_path_buf(),
                                scroll: 0,
                            });
                        }
                    }
                    if !is_remote {
                        match self.documents[tab].save() {
                            Ok(()) => self.status_message = Some("Saved".to_string()),
                            Err(e) => self.status_message = Some(format!("Save failed: {}", e)),
                        }
                    }
                }
            }
            Action::SaveAs => {
                self.clear_ai_inline_state();
                self.completion_list = None;
                self.path_prompt_after_save = None;
                self.modal = Some(ModalState::prompt_path(PathPromptMode::SaveAs));
            }
            Action::NewFile => {
                let doc = Document::new();
                let hl = SyntaxHighlighter::new(&doc.language);
                self.documents.push(doc);
                self.highlighters.push(hl);
                self.active_tab = self.documents.len() - 1;
            }
            Action::CloseBuffer => {
                if self.documents.len() > 1 {
                    let is_modified = self.documents[tab].is_modified();
                    if is_modified {
                        let name = self.documents[tab].display_name();
                        self.save_confirm_pending = Some(SaveConfirmPending::CloseTab(tab));
                        self.clear_ai_inline_state();
                        self.completion_list = None;
                        self.modal = Some(ModalState::save_confirm(&name));
                    } else {
                        self.documents.remove(tab);
                        self.highlighters.remove(tab);
                        if self.active_tab >= self.documents.len() {
                            self.active_tab = self.documents.len().saturating_sub(1);
                        }
                    }
                } else if !self.documents[tab].is_modified() {
                    self.should_quit = true;
                } else {
                    let name = self.documents[tab].display_name();
                    self.save_confirm_pending = Some(SaveConfirmPending::CloseTab(tab));
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                    self.modal = Some(ModalState::save_confirm(&name));
                }
            }
            Action::OpenFile => {
                self.clear_ai_inline_state();
                self.completion_list = None;
                self.modal = Some(ModalState::prompt_path(PathPromptMode::Open));
            }

            // === Search ===
            Action::Find => {
                self.clear_ai_inline_state();
                self.completion_list = None;
                self.close_ai_panel_overlay();
                self.open_tabs_palette.close();
                self.open_tabs_debounce_at = None;
                if let Some(ref mut m) = self.modal {
                    if m.kind == ModalKind::Find {
                        m.find_bar_focus = FindBarFocus::Query;
                        self.find_bar_anim_frames = 4;
                    } else {
                        self.modal = Some(ModalState::find());
                    }
                } else {
                    self.modal = Some(ModalState::find());
                }
            }
            Action::FindReplace => {
                self.clear_ai_inline_state();
                self.completion_list = None;
                self.close_ai_panel_overlay();
                self.open_tabs_palette.close();
                self.open_tabs_debounce_at = None;
                self.modal = Some(ModalState::find_replace());
            }
            Action::FindInOpenTabs => {
                if !self.settings.find_in_open_tabs_enabled {
                    self.status_message = Some(
                        "Find in Open Tabs is disabled (find_in_open_tabs_enabled = false)."
                            .to_string(),
                    );
                } else {
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                    if self.open_tabs_palette.visible {
                        self.open_tabs_palette.close();
                        self.open_tabs_debounce_at = None;
                    } else {
                        self.open_find_in_open_tabs_palette();
                    }
                }
            }
            Action::FindNext => {
                if let Some(m) = self.search.next_match().cloned() {
                    let doc = &mut self.documents[tab];
                    let line = doc.buffer.char_to_line(m.start);
                    let line_start = doc.buffer.line_to_char(line);
                    let col = m.start - line_start;
                    doc.cursor.goto(line, col, &doc.buffer);
                }
            }
            Action::FindPrev => {
                if let Some(m) = self.search.prev_match().cloned() {
                    let doc = &mut self.documents[tab];
                    let line = doc.buffer.char_to_line(m.start);
                    let line_start = doc.buffer.line_to_char(line);
                    let col = m.start - line_start;
                    doc.cursor.goto(line, col, &doc.buffer);
                }
            }
            Action::GoToLine => {
                self.clear_ai_inline_state();
                self.completion_list = None;
                self.close_ai_panel_overlay();
                self.outline_palette.close();
                self.open_tabs_palette.close();
                self.open_tabs_debounce_at = None;
                self.modal = Some(ModalState::goto_line());
            }
            Action::GoToSymbol => {
                if !self.settings.outline_enabled {
                    self.status_message =
                        Some("Go to Symbol is disabled (outline_enabled = false).".to_string());
                } else {
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                    if self.outline_palette.visible {
                        self.outline_palette.close();
                    } else {
                        self.open_outline_palette();
                    }
                }
            }
            Action::GoToMatchingBracket => {
                self.go_to_matching_bracket_action();
            }
            Action::EscapeSearch => {
                self.search.clear();
                self.clear_ai_inline_state();
                self.completion_list = None;
            }

            // === View ===
            Action::ToggleFileTree => {
                self.show_file_tree = !self.show_file_tree;
            }
            Action::ToggleAiPanel => {
                #[cfg(not(feature = "ai"))]
                {
                    self.status_message = Some(
                        "This build was compiled without AI. Install with: cargo install termedit --features ai"
                            .to_string(),
                    );
                }
                #[cfg(feature = "ai")]
                {
                    if !self.settings.ai_enabled {
                        self.status_message =
                            Some("AI is disabled for this run (see --no-ai or ai_enabled).".to_string());
                    } else if self.ai_panel.visible {
                        self.sync_ai_chat_model_setting();
                        self.close_ai_panel_overlay();
                    } else {
                        self.command_palette.close();
                        self.outline_palette.close();
                        self.open_tabs_palette.close();
                        self.open_tabs_debounce_at = None;
                        self.ai_panel.open();
                    }
                }
            }
            Action::AiInsertLastReply => {
                #[cfg(feature = "ai")]
                if self.ai_panel.visible {
                    self.insert_last_ai_reply_at_cursor();
                }
            }
            Action::AiBrainstorm => {
                #[cfg(not(feature = "ai"))]
                {
                    self.status_message = Some(
                        "This build was compiled without AI. Install with: cargo install termedit --features ai"
                            .to_string(),
                    );
                }
                #[cfg(feature = "ai")]
                {
                    if !self.settings.ai_enabled {
                        self.status_message =
                            Some("AI is disabled for this run (see --no-ai or ai_enabled).".to_string());
                    } else {
                        self.command_palette.close();
                        self.outline_palette.close();
                        self.open_tabs_palette.close();
                        self.open_tabs_debounce_at = None;
                        let tab = self.active_tab;
                        let doc = &self.documents[tab];
                        let file_display = doc.display_name().to_string();
                        let language = doc.language.clone();
                        self.ai_panel.open();
                        self.ai_panel.input =
                            gemini_chat::brainstorm_user_prompt(&file_display, &language);
                        self.ai_panel.error = None;
                        self.ai_panel.stick_transcript_to_bottom = true;
                    }
                }
            }
            Action::CommandPalette => {
                if self.command_palette.visible {
                    self.command_palette.close();
                } else {
                    self.outline_palette.close();
                    self.open_tabs_palette.close();
                    self.open_tabs_debounce_at = None;
                    self.close_ai_panel_overlay();
                    self.command_palette.open();
                }
            }

            // === Tabs ===
            Action::NextTab => {
                if !self.documents.is_empty() {
                    self.active_tab = (self.active_tab + 1) % self.documents.len();
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                }
            }
            Action::PrevTab => {
                if !self.documents.is_empty() {
                    self.active_tab = if self.active_tab == 0 {
                        self.documents.len() - 1
                    } else {
                        self.active_tab - 1
                    };
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                }
            }
            Action::GoToTab(idx) => {
                if !self.documents.is_empty() {
                    let idx = idx.min(self.documents.len().saturating_sub(1));
                    self.active_tab = idx;
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                }
            }

            // === App ===
            Action::Quit => {
                let has_unsaved = self.documents.iter().any(|d| d.is_modified());
                if has_unsaved {
                    self.save_confirm_pending = Some(SaveConfirmPending::Quit);
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                    self.modal = Some(ModalState::save_confirm("unsaved files"));
                } else {
                    self.should_quit = true;
                }
            }
            Action::ForceQuit => {
                self.should_quit = true;
            }
            Action::Deploy => {
                #[cfg(feature = "ssh")]
                if let Some(ctx) = &self.ssh_context {
                    match ctx.rt.block_on(ctx.deployer.execute_deploy()) {
                        Ok(msg) => {
                            self.status_message = Some(msg);
                        }
                        Err(e) => {
                            self.status_message = Some(e.to_string());
                        }
                    }
                } else {
                    self.status_message = Some("Deploy is only available in SSH mode.".to_string());
                }
                #[cfg(not(feature = "ssh"))]
                {
                    self.status_message = Some("Deploy requires the `ssh` feature.".to_string());
                }
            }

            _ => {}
        }

        self.sync_bracket_highlight();

        // Invalidate ghost suggestion when cursor moved to another line
        #[cfg(feature = "ai")]
        {
            if let Some((trigger_line, _)) = self.ghost_trigger_pos {
                if self.documents[self.active_tab].cursor.line != trigger_line {
                    self.clear_ai_inline_state();
                    self.completion_list = None;
                }
            }
        }

        // Ensure cursor visibility
        self.documents[self.active_tab].ensure_cursor_visible(vh, vw);
    }

    fn apply_find_from_modal(&mut self, immediate: bool) {
        let Some(ref modal) = self.modal else {
            return;
        };
        if !matches!(modal.kind, ModalKind::Find | ModalKind::FindReplace) {
            return;
        }
        if modal.kind == ModalKind::FindReplace
            && modal.find_replace_focus == FindReplaceFocus::Replace
        {
            return;
        }
        let config = search_config_from_modal(modal);
        let tab = self.active_tab;
        let rope = &self.documents[tab].buffer.rope;
        if config.is_regex && !immediate {
            self.find_debounce_at = Some(Instant::now() + Duration::from_millis(120));
            self.find_pending_config = Some(config);
        } else {
            self.find_debounce_at = None;
            self.find_pending_config = None;
            self.search.find(config, rope);
        }
    }

    fn flush_find_debounce_now(&mut self) {
        if let Some(cfg) = self.find_pending_config.take() {
            self.find_debounce_at = None;
            let tab = self.active_tab;
            let rope = &self.documents[tab].buffer.rope;
            self.search.find(cfg, rope);
        }
    }

    fn flush_find_debounce_if_ready(&mut self) -> bool {
        let Some(deadline) = self.find_debounce_at else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }
        self.find_debounce_at = None;
        let Some(cfg) = self.find_pending_config.take() else {
            return false;
        };
        let tab = self.active_tab;
        let rope = &self.documents[tab].buffer.rope;
        self.search.find(cfg, rope);
        true
    }

    fn tick_find_bar_animation(&mut self) -> bool {
        if self.find_bar_anim_frames == 0 {
            return false;
        }
        if !matches!(
            self.modal,
            Some(ref m) if m.kind == ModalKind::Find || m.kind == ModalKind::FindReplace
        ) {
            self.find_bar_anim_frames = 0;
            return false;
        }
        self.find_bar_anim_frames -= 1;
        true
    }

    fn jump_to_search_match(&mut self, start_char: usize) {
        let tab = self.active_tab;
        let doc = &mut self.documents[tab];
        let line = doc.buffer.char_to_line(start_char);
        let line_start = doc.buffer.line_to_char(line);
        let col = start_char - line_start;
        doc.cursor.goto(line, col, &doc.buffer);
        doc.ensure_cursor_visible(self.viewport_height, self.viewport_width);
    }

    fn modal_find_next(&mut self) {
        self.flush_find_debounce_now();
        if let Some(m) = self.search.next_match() {
            let s = m.start;
            self.jump_to_search_match(s);
        }
    }

    fn modal_find_prev(&mut self) {
        self.flush_find_debounce_now();
        if let Some(m) = self.search.prev_match() {
            let s = m.start;
            self.jump_to_search_match(s);
        }
    }

    fn modal_on_find_row(&self) -> bool {
        self.modal.as_ref().is_some_and(|m| {
            m.kind == ModalKind::Find
                || (m.kind == ModalKind::FindReplace
                    && m.find_replace_focus == FindReplaceFocus::Find)
        })
    }

    fn modal_chrome_left_right(&mut self, dir_right: bool) {
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        let replace_only = modal.kind == ModalKind::FindReplace
            && modal.find_replace_focus == FindReplaceFocus::Replace;
        if replace_only {
            if dir_right {
                modal.cursor_right();
            } else {
                modal.cursor_left();
            }
            return;
        }
        if modal.find_query_focused() {
            if dir_right {
                modal.cursor_right();
            } else {
                modal.cursor_left();
            }
            return;
        }
        modal.find_bar_focus = if dir_right {
            modal.find_bar_focus.next()
        } else {
            modal.find_bar_focus.prev()
        };
        self.find_bar_anim_frames = 4;
    }

    /// Handle keyboard input when a modal is active.
    fn handle_modal_key(&mut self, key: KeyEvent) {
        let tab = self.active_tab;

        let modal_kind = match &self.modal {
            Some(m) => m.kind.clone(),
            None => return,
        };

        match key.code {
            KeyCode::Esc => {
                if modal_kind == ModalKind::SaveConfirm {
                    self.save_confirm_pending = None;
                }
                if matches!(modal_kind, ModalKind::PromptPath(_)) {
                    self.path_prompt_after_save = None;
                }
                self.modal = None;
                return;
            }
            KeyCode::F(3) if matches!(modal_kind, ModalKind::Find | ModalKind::FindReplace) => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.modal_find_prev();
                } else {
                    self.modal_find_next();
                }
                return;
            }
            KeyCode::Up if matches!(modal_kind, ModalKind::Find | ModalKind::FindReplace) => {
                if self.modal_on_find_row() {
                    self.modal_find_prev();
                    return;
                }
            }
            KeyCode::Down if matches!(modal_kind, ModalKind::Find | ModalKind::FindReplace) => {
                if self.modal_on_find_row() {
                    self.modal_find_next();
                    return;
                }
            }
            KeyCode::Tab if modal_kind == ModalKind::Find => {
                if let Some(m) = self.modal.as_mut() {
                    m.find_bar_focus = m.find_bar_focus.next();
                }
                self.find_bar_anim_frames = 4;
                return;
            }
            KeyCode::BackTab if modal_kind == ModalKind::Find => {
                if let Some(m) = self.modal.as_mut() {
                    m.find_bar_focus = m.find_bar_focus.prev();
                }
                self.find_bar_anim_frames = 4;
                return;
            }
            KeyCode::Tab if modal_kind == ModalKind::FindReplace => {
                let landed_on_find = if let Some(m) = self.modal.as_mut() {
                    let was_replace = m.find_replace_focus == FindReplaceFocus::Replace;
                    find_replace_tab(m);
                    was_replace && m.find_replace_focus == FindReplaceFocus::Find
                } else {
                    false
                };
                if landed_on_find {
                    self.apply_find_from_modal(true);
                }
                self.find_bar_anim_frames = 4;
                return;
            }
            KeyCode::BackTab if modal_kind == ModalKind::FindReplace => {
                let landed_on_find = if let Some(m) = self.modal.as_mut() {
                    let was_replace = m.find_replace_focus == FindReplaceFocus::Replace;
                    find_replace_backtab(m);
                    was_replace && m.find_replace_focus == FindReplaceFocus::Find
                } else {
                    false
                };
                if landed_on_find {
                    self.apply_find_from_modal(true);
                }
                self.find_bar_anim_frames = 4;
                return;
            }
            KeyCode::Enter => {
                let ctrl_enter = key.modifiers.contains(KeyModifiers::CONTROL);
                if matches!(modal_kind, ModalKind::Find | ModalKind::FindReplace) {
                    if let Some(m) = self.modal.as_ref() {
                        let on_find_row = modal_kind == ModalKind::Find
                            || m.find_replace_focus == FindReplaceFocus::Find;
                        if on_find_row && m.find_bar_focus == FindBarFocus::Close && !ctrl_enter {
                            self.modal = None;
                            return;
                        }
                    }
                }
                match &modal_kind {
                    ModalKind::Find => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        self.flush_find_debounce_now();
                        if shift {
                            self.modal_find_prev();
                        } else {
                            self.modal_find_next();
                        }
                    }
                    ModalKind::FindReplace => {
                        let (repl, focus) = self
                            .modal
                            .as_ref()
                            .map(|m| (m.replace_input.clone(), m.find_replace_focus))
                            .unwrap_or_default();
                        match focus {
                            FindReplaceFocus::Find => {
                                let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                                self.flush_find_debounce_now();
                                if shift {
                                    self.modal_find_prev();
                                } else {
                                    self.modal_find_next();
                                }
                            }
                            FindReplaceFocus::Replace => {
                                let cfg = self
                                    .modal
                                    .as_ref()
                                    .map(search_config_from_modal)
                                    .expect("modal");
                                if ctrl_enter {
                                    let rope = &self.documents[tab].buffer.rope;
                                    self.search.find(cfg.clone(), rope);
                                    let matches_snapshot = self.search.matches.clone();
                                    let n = self.documents[tab]
                                        .replace_all_matches(&matches_snapshot, &repl);
                                    self.modal = None;
                                    self.status_message =
                                        Some(format!("Replaced {} occurrence(s)", n));
                                } else {
                                    let rope = &self.documents[tab].buffer.rope;
                                    self.search.find(cfg.clone(), rope);
                                    let Some(m) = self.search.current().cloned() else {
                                        self.status_message = Some("No match".into());
                                        return;
                                    };
                                    self.documents[tab].replace_char_range(m.start, m.end, &repl);
                                    let rope = &self.documents[tab].buffer.rope;
                                    self.search.find(cfg, rope);
                                    if let Some(nm) = self.search.current().cloned() {
                                        let doc = &mut self.documents[tab];
                                        let line = doc.buffer.char_to_line(nm.start);
                                        let line_start = doc.buffer.line_to_char(line);
                                        let col = nm.start - line_start;
                                        doc.cursor.goto(line, col, &doc.buffer);
                                        doc.ensure_cursor_visible(
                                            self.viewport_height,
                                            self.viewport_width,
                                        );
                                    } else {
                                        self.status_message = Some("No more matches".into());
                                    }
                                }
                            }
                        }
                    }
                    ModalKind::GoToLine => {
                        let input = self
                            .modal
                            .as_ref()
                            .map(|m| m.input.clone())
                            .unwrap_or_default();
                        self.modal = None;
                        if let Ok(line) = input.parse::<usize>() {
                            let target = line.saturating_sub(1);
                            let doc = &mut self.documents[tab];
                            let line_count = doc.buffer.line_count();
                            let target = target.min(line_count.saturating_sub(1));
                            doc.cursor.goto(target, 0, &doc.buffer);
                            doc.ensure_cursor_visible(self.viewport_height, self.viewport_width);
                        }
                    }
                    ModalKind::SaveConfirm => {
                        self.modal = None;
                        self.save_confirm_pending = None;
                    }
                    ModalKind::PromptPath(mode) => {
                        let mode = *mode;
                        let path_str = self
                            .modal
                            .as_ref()
                            .map(|m| m.input.clone())
                            .unwrap_or_default();
                        self.submit_path_prompt(mode, &path_str);
                    }
                }
                return;
            }
            KeyCode::Char('y') if modal_kind == ModalKind::SaveConfirm => {
                let pending = match self.save_confirm_pending.take() {
                    Some(p) => p,
                    None => return,
                };
                self.modal = None;
                match pending {
                    SaveConfirmPending::CloseTab(t) => {
                        if self.documents.get(t).is_none() {
                            return;
                        }
                        if self.documents[t].buffer.file_path.is_some() {
                            match self.documents[t].save() {
                                Ok(()) => self.remove_tab_at(t),
                                Err(e) => self.status_message = Some(format!("Save failed: {}", e)),
                            }
                        } else {
                            self.active_tab = t;
                            self.path_prompt_after_save = Some(PathAfterSave::CloseTab(t));
                            self.modal = Some(ModalState::prompt_path(PathPromptMode::SaveAs));
                        }
                    }
                    SaveConfirmPending::Quit => {
                        if let Err(e) = self.save_modified_with_paths() {
                            self.status_message = Some(e);
                            return;
                        }
                        if let Some(i) = self.documents.iter().position(|d| d.is_modified()) {
                            self.active_tab = i;
                            self.path_prompt_after_save = Some(PathAfterSave::QuitSaveAll);
                            self.modal = Some(ModalState::prompt_path(PathPromptMode::SaveAs));
                        } else {
                            self.should_quit = true;
                        }
                    }
                }
                return;
            }
            KeyCode::Char('n') if modal_kind == ModalKind::SaveConfirm => {
                if let Some(pending) = self.save_confirm_pending.take() {
                    match pending {
                        SaveConfirmPending::CloseTab(t) => self.remove_tab_at(t),
                        SaveConfirmPending::Quit => self.should_quit = true,
                    }
                }
                self.modal = None;
                return;
            }
            KeyCode::Char(' ') => {
                if matches!(modal_kind, ModalKind::Find | ModalKind::FindReplace) {
                    if let Some(m) = self.modal.as_mut() {
                        let on_find_row = modal_kind == ModalKind::Find
                            || m.find_replace_focus == FindReplaceFocus::Find;
                        if on_find_row && !m.find_query_focused() {
                            match m.find_bar_focus {
                                FindBarFocus::ToggleCase => {
                                    m.match_case = !m.match_case;
                                }
                                FindBarFocus::ToggleWord => {
                                    m.whole_word = !m.whole_word;
                                }
                                FindBarFocus::ToggleRegex => {
                                    m.use_regex = !m.use_regex;
                                }
                                FindBarFocus::Close => {
                                    self.modal = None;
                                    return;
                                }
                                FindBarFocus::Prev => {
                                    self.modal_find_prev();
                                    return;
                                }
                                FindBarFocus::Next => {
                                    self.modal_find_next();
                                    return;
                                }
                                FindBarFocus::Query => {}
                            }
                            if matches!(
                                m.find_bar_focus,
                                FindBarFocus::ToggleCase
                                    | FindBarFocus::ToggleWord
                                    | FindBarFocus::ToggleRegex
                            ) {
                                self.apply_find_from_modal(true);
                                self.find_bar_anim_frames = 4;
                                return;
                            }
                        }
                    }
                }
                if let Some(modal) = self.modal.as_mut() {
                    modal.insert_char(' ');
                }
                if modal_kind == ModalKind::FindReplace {
                    let fr = self
                        .modal
                        .as_ref()
                        .map(|m| m.find_replace_focus)
                        .unwrap_or(FindReplaceFocus::Find);
                    if fr == FindReplaceFocus::Find {
                        self.apply_find_from_modal(false);
                    }
                } else if modal_kind == ModalKind::Find {
                    self.apply_find_from_modal(false);
                }
            }
            KeyCode::Char(c) => {
                if let Some(modal) = self.modal.as_mut() {
                    modal.insert_char(c);
                }
                if modal_kind == ModalKind::FindReplace {
                    let fr = self
                        .modal
                        .as_ref()
                        .map(|m| m.find_replace_focus)
                        .unwrap_or(FindReplaceFocus::Find);
                    if fr == FindReplaceFocus::Find {
                        self.apply_find_from_modal(false);
                    }
                } else if modal_kind == ModalKind::Find {
                    self.apply_find_from_modal(false);
                }
            }
            KeyCode::Backspace => {
                if let Some(modal) = self.modal.as_mut() {
                    modal.backspace();
                }
                if modal_kind == ModalKind::FindReplace {
                    let fr = self
                        .modal
                        .as_ref()
                        .map(|m| m.find_replace_focus)
                        .unwrap_or(FindReplaceFocus::Find);
                    if fr == FindReplaceFocus::Find {
                        self.apply_find_from_modal(false);
                    }
                } else if modal_kind == ModalKind::Find {
                    self.apply_find_from_modal(false);
                }
            }
            KeyCode::Left => {
                self.modal_chrome_left_right(false);
            }
            KeyCode::Right => {
                self.modal_chrome_left_right(true);
            }
            _ => {}
        }
    }

    /// Handle mouse events.
    fn handle_mouse(&mut self, event: crossterm::event::MouseEvent) {
        let tab = self.active_tab;
        let gutter_width = 5u16;

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if event.column >= gutter_width {
                    let doc = &mut self.documents[tab];
                    let col = (event.column - gutter_width) as usize + doc.scroll_x;
                    let line = (event.row as usize).saturating_sub(1) + doc.scroll_y;
                    doc.cursor.goto(line, col, &doc.buffer);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if event.column >= gutter_width {
                    let doc = &mut self.documents[tab];
                    let col = (event.column - gutter_width) as usize + doc.scroll_x;
                    let line = (event.row as usize).saturating_sub(1) + doc.scroll_y;
                    doc.cursor.start_selection(SelectionMode::Char);
                    let max_line = doc.buffer.line_count().saturating_sub(1);
                    doc.cursor.line = line.min(max_line);
                    let line_len = doc.buffer.line_len(doc.cursor.line);
                    doc.cursor.col = col.min(line_len);
                    doc.cursor.col_target = doc.cursor.col;
                }
            }
            MouseEventKind::ScrollUp => {
                self.documents[tab].scroll_y =
                    self.documents[tab].scroll_y.saturating_sub(3);
            }
            MouseEventKind::ScrollDown => {
                let max = self.documents[tab]
                    .buffer
                    .line_count()
                    .saturating_sub(1);
                self.documents[tab].scroll_y =
                    (self.documents[tab].scroll_y + 3).min(max);
            }
            _ => {}
        }
        self.sync_bracket_highlight();
    }

    /// Render the entire UI.
    fn render(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();
        self.viewport_height = size.height.saturating_sub(2) as usize;
        self.viewport_width = size.width as usize;

        // Ensure cursor is visible
        let vh = self.viewport_height;
        let vw = self.viewport_width;
        self.documents[self.active_tab].ensure_cursor_visible(vh, vw);

        // Layout: [TabBar (1)] [FileTree? | Editor] [StatusBar (1)]
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(size);

        // Tab bar
        let tab_infos: Vec<TabInfo> = self
            .documents
            .iter()
            .map(|d| TabInfo {
                name: d.display_name(),
                modified: d.is_modified(),
            })
            .collect();
        frame.render_widget(
            TabBar::new(&tab_infos, self.active_tab, &self.theme),
            main_chunks[0],
        );

        // Editor area
        let editor_area = if self.show_file_tree {
            let horiz = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(FileTree::width()),
                    Constraint::Min(1),
                ])
                .split(main_chunks[1]);

            frame.render_widget(FileTree::new(&self.theme, true), horiz[0]);
            horiz[1]
        } else {
            main_chunks[1]
        };

        // Editor pane
        let doc = &self.documents[self.active_tab];
        let hl = &self.highlighters[self.active_tab];
        let ghost_text: Option<&str> = {
            #[cfg(feature = "ai")]
            {
                if self.ai_pending && self.ghost_suggestion.is_none() {
                    Some("...")
                } else {
                    self.ghost_suggestion.as_deref()
                }
            }
            #[cfg(not(feature = "ai"))]
            {
                None
            }
        };
        let completion_dropdown = self.completion_list.as_ref().map(|c| (c.items.as_slice(), c.selected));
        let show_match_strip = self.search.match_count() > 0;
        let bracket_hl = self.bracket_highlight;
        frame.render_widget(
            EditorPane::new(doc, &self.theme, hl, &self.search)
                .ghost_text(ghost_text)
                .completion_dropdown(completion_dropdown)
                .match_strip(show_match_strip)
                .bracket_highlight(bracket_hl),
            editor_area,
        );

        // Modal overlay
        #[cfg(feature = "ssh")]
        if let Some(diff_state) = &self.ssh_diff_state {
            frame.render_widget(
                crate::feature::ssh::ui::SshDiffModalWidget {
                    diff: &diff_state.diff,
                    filename: &diff_state.filename,
                    scroll: diff_state.scroll,
                    theme: &self.theme,
                },
                editor_area,
            );
        } else if let Some(ref modal) = self.modal {
            let search_status =
                if modal.kind == ModalKind::Find || modal.kind == ModalKind::FindReplace {
                    Some(self.search.find_bar_status())
                } else {
                    None
                };
            frame.render_widget(
                ModalWidget::new(modal, &self.theme)
                    .search_status(search_status)
                    .find_bar_anim(self.find_bar_anim_frames),
                editor_area,
            );
        }
        #[cfg(not(feature = "ssh"))]
        if let Some(ref modal) = self.modal {
            let search_status =
                if modal.kind == ModalKind::Find || modal.kind == ModalKind::FindReplace {
                    Some(self.search.find_bar_status())
                } else {
                    None
                };
            frame.render_widget(
                ModalWidget::new(modal, &self.theme)
                    .search_status(search_status)
                    .find_bar_anim(self.find_bar_anim_frames),
                editor_area,
            );
        }

        if self.command_palette.visible {
            frame.render_widget(
                CommandPaletteWidget {
                    state: &self.command_palette,
                    theme: &self.theme,
                },
                editor_area,
            );
        }

        if self.outline_palette.visible {
            frame.render_widget(
                OutlinePaletteWidget {
                    state: &self.outline_palette,
                    theme: &self.theme,
                },
                editor_area,
            );
        }

        if self.open_tabs_palette.visible {
            frame.render_widget(
                OpenTabsPaletteWidget {
                    state: &self.open_tabs_palette,
                    theme: &self.theme,
                },
                editor_area,
            );
        }

        #[cfg(feature = "ai")]
        if self.ai_panel.visible {
            frame.render_widget(
                AiPanelWidget {
                    state: &self.ai_panel,
                    theme: &self.theme,
                },
                editor_area,
            );
        }

        // Status bar
        let search_status =
            if self.search.match_count() > 0 || self.search.last_error.is_some() {
                Some(self.search.find_bar_status())
            } else {
                None
            };
        let doc = &self.documents[self.active_tab];
        let mut status = StatusBar::new(doc, &self.theme)
            .search_status(search_status)
            .message(self.status_message.clone());
        if self.documents.len() > 1 {
            status = status.tab_hint(self.active_tab + 1, self.documents.len());
        }
        frame.render_widget(status, main_chunks[2]);
    }
}
