/// Central application state and event loop.
///
/// The `App` struct holds all editor state and orchestrates the event loop:
/// terminal events → action mapping → state update → render.

use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode,
    MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;

use crate::config::keymap::{self, Action};
use crate::config::settings::Settings;
use crate::config::theme::Theme;
use crate::core::cursor::SelectionMode;
use crate::core::document::Document;
use crate::feature::ai_completion::{self, AiContext};
use crate::feature::completion;
use crate::feature::search::{Search, SearchConfig};
use crate::feature::session::{self, SessionState};
use crate::feature::syntax::SyntaxHighlighter;
use crate::ui::editor_pane::EditorPane;
use crate::ui::file_tree::FileTree;
use crate::ui::modal::{ModalKind, ModalState, ModalWidget};
use crate::ui::status_bar::StatusBar;
use crate::ui::tab_bar::{TabBar, TabInfo};

/// Pending action to run after the user answers the save-confirm modal.
#[derive(Debug, Clone)]
pub enum SaveConfirmPending {
    CloseTab(usize),
    Quit,
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
    /// Inline AI suggestion (ghost text) to show after cursor; Tab accepts.
    ghost_suggestion: Option<String>,
    /// Cursor (line, col) when suggestion was computed; clear suggestion if cursor moves line.
    ghost_trigger_pos: Option<(usize, usize)>,
    /// Channel to receive AI completion results (generation, suggestion).
    ai_rx: mpsc::Receiver<(u64, Option<String>)>,
    ai_tx: mpsc::Sender<(u64, Option<String>)>,
    /// Channel to send (generation, context) to the single AI worker.
    ai_request_tx: mpsc::Sender<(u64, AiContext)>,
    /// Generation ID to ignore stale AI responses.
    ai_generation: u64,
    /// When we last edited (for debounced AI request).
    last_ai_edit: Option<Instant>,
    /// Generation for which we already sent an AI request.
    ai_request_sent_for: Option<u64>,
    /// True when we sent an AI request for current generation and have not yet received a result.
    ai_pending: bool,
    /// Inline completion dropdown: list of items, selected index, and prefix length for accept.
    completion_list: Option<CompletionList>,
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
        let (ai_tx, ai_rx) = mpsc::channel();
        let (ai_request_tx, ai_request_rx) = mpsc::channel();
        let debounce_ms = settings.ai_debounce_ms;
        ai_completion::spawn_ai_worker(ai_request_rx, ai_tx.clone(), debounce_ms);

        Self {
            documents: vec![doc],
            active_tab: 0,
            highlighters: vec![hl],
            search: Search::new(),
            modal: None,
            save_confirm_pending: None,
            settings,
            theme,
            show_file_tree: false,
            should_quit: false,
            status_message: None,
            dirty: true,
            viewport_height: 24,
            viewport_width: 80,
            ghost_suggestion: None,
            ghost_trigger_pos: None,
            ai_rx,
            ai_tx,
            ai_request_tx,
            ai_generation: 0,
            last_ai_edit: None,
            ai_request_sent_for: None,
            ai_pending: false,
            completion_list: None,
        }
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
        loop {
            if self.dirty {
                terminal.draw(|frame| self.render(frame))?;
                self.dirty = false;
            }

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
                        let context_before: String = (0..doc.cursor.line)
                            .rev()
                            .take(30)
                            .map(|i| doc.buffer.line_text(i))
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>()
                            .join("\n");
                        let context = AiContext {
                            line_prefix,
                            context_before,
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

            if event::poll(Duration::from_millis(16))? {
                let evt = event::read()?;
                self.handle_event(evt);
                self.dirty = true;

                if self.should_quit {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Handle a terminal event.
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key_event) => {
                if self.modal.is_some() {
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
                                self.ghost_suggestion = None;
                                self.ghost_trigger_pos = None;
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
                    self.ghost_trigger_pos = Some((doc.cursor.line, doc.cursor.col));
                    self.last_ai_edit = Some(Instant::now());
                    self.ai_generation = self.ai_generation.wrapping_add(1);
                    self.ghost_suggestion = completion::suggest(doc);
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
                if let Some(text) = self.ghost_suggestion.take() {
                    self.ghost_trigger_pos = None;
                    self.documents[tab].insert_text(&text);
                } else {
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
                    self.ghost_trigger_pos = Some((doc.cursor.line, doc.cursor.col));
                    self.last_ai_edit = Some(Instant::now());
                    self.ai_generation = self.ai_generation.wrapping_add(1);
                    self.ghost_suggestion = completion::suggest(doc);
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
                    self.ghost_trigger_pos = Some((doc.cursor.line, doc.cursor.col));
                    self.last_ai_edit = Some(Instant::now());
                    self.ai_generation = self.ai_generation.wrapping_add(1);
                    self.ghost_suggestion = completion::suggest(doc);
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

            // === Clipboard ===
            Action::Copy => {
                let doc = &self.documents[tab];
                if let Some(text) = doc.cursor.selected_text(&doc.buffer) {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(text);
                        self.status_message = Some("Copied".to_string());
                    }
                }
            }
            Action::Cut => {
                let doc = &self.documents[tab];
                if let Some(text) = doc.cursor.selected_text(&doc.buffer) {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(text);
                    }
                }
                self.documents[tab].delete_selection();
                self.status_message = Some("Cut".to_string());
            }
            Action::Paste => {
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

            // === File Operations ===
            Action::Save => {
                match self.documents[tab].save() {
                    Ok(()) => self.status_message = Some("Saved".to_string()),
                    Err(e) => self.status_message = Some(format!("Save failed: {}", e)),
                }
            }
            Action::SaveAs => {
                // TODO: implement save-as dialog
                self.status_message = Some("Save As: not yet implemented".to_string());
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
                        self.ghost_suggestion = None;
                        self.ghost_trigger_pos = None;
                        self.ai_pending = false;
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
                    self.ghost_suggestion = None;
                    self.ghost_trigger_pos = None;
                    self.ai_pending = false;
                    self.completion_list = None;
                    self.modal = Some(ModalState::save_confirm(&name));
                }
            }
            Action::OpenFile => {
                // TODO: implement file open dialog
                self.status_message = Some("Open: not yet implemented".to_string());
            }

            // === Search ===
            Action::Find => {
                self.ghost_suggestion = None;
                self.ghost_trigger_pos = None;
                self.ai_pending = false;
                self.completion_list = None;
                self.modal = Some(ModalState::find());
            }
            Action::FindReplace => {
                self.ghost_suggestion = None;
                self.ghost_trigger_pos = None;
                self.ai_pending = false;
                self.completion_list = None;
                self.modal = Some(ModalState::find_replace());
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
                self.ghost_suggestion = None;
                self.ghost_trigger_pos = None;
                self.ai_pending = false;
                self.completion_list = None;
                self.modal = Some(ModalState::goto_line());
            }
            Action::EscapeSearch => {
                self.search.clear();
                self.ghost_suggestion = None;
                self.ghost_trigger_pos = None;
                self.ai_pending = false;
                self.completion_list = None;
            }

            // === View ===
            Action::ToggleFileTree => {
                self.show_file_tree = !self.show_file_tree;
            }
            Action::ToggleAiPanel | Action::CommandPalette => {
                // TODO: future features
            }

            // === Tabs ===
            Action::NextTab => {
                if !self.documents.is_empty() {
                    self.active_tab = (self.active_tab + 1) % self.documents.len();
                    self.ghost_suggestion = None;
                    self.ghost_trigger_pos = None;
                    self.ai_pending = false;
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
                    self.ghost_suggestion = None;
                    self.ghost_trigger_pos = None;
                    self.ai_pending = false;
                    self.completion_list = None;
                }
            }

            // === App ===
            Action::Quit => {
                let has_unsaved = self.documents.iter().any(|d| d.is_modified());
                if has_unsaved {
                    self.save_confirm_pending = Some(SaveConfirmPending::Quit);
                    self.ghost_suggestion = None;
                    self.ghost_trigger_pos = None;
                    self.ai_pending = false;
                    self.completion_list = None;
                    self.modal = Some(ModalState::save_confirm("unsaved files"));
                } else {
                    self.should_quit = true;
                }
            }
            Action::ForceQuit => {
                self.should_quit = true;
            }

            _ => {}
        }

        // Invalidate ghost suggestion when cursor moved to another line
        if let Some((trigger_line, _)) = self.ghost_trigger_pos {
            if self.documents[self.active_tab].cursor.line != trigger_line {
                self.ghost_suggestion = None;
                self.ghost_trigger_pos = None;
                self.ai_pending = false;
                self.completion_list = None;
            }
        }

        // Ensure cursor visibility
        self.documents[self.active_tab].ensure_cursor_visible(vh, vw);
    }

    /// Handle keyboard input when a modal is active.
    fn handle_modal_key(&mut self, key: crossterm::event::KeyEvent) {
        let tab = self.active_tab;

        // Destructure modal to check its kind without holding a borrow
        let modal_kind = match &self.modal {
            Some(m) => m.kind.clone(),
            None => return,
        };

        match key.code {
            KeyCode::Esc => {
                if modal_kind == ModalKind::SaveConfirm {
                    self.save_confirm_pending = None;
                }
                self.modal = None;
                return;
            }
            KeyCode::Enter => {
                match modal_kind {
                    ModalKind::Find => {
                        let pattern = self.modal.as_ref().unwrap().input.clone();
                        self.modal = None;
                        self.execute_search(&pattern);
                    }
                    ModalKind::FindReplace => {
                        self.modal = None;
                    }
                    ModalKind::GoToLine => {
                        let input = self.modal.as_ref().unwrap().input.clone();
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
                }
                return;
            }
            KeyCode::Char('y') if modal_kind == ModalKind::SaveConfirm => {
                let _ = self.documents[tab].save();
                if let Some(pending) = self.save_confirm_pending.take() {
                    match pending {
                        SaveConfirmPending::CloseTab(t) => {
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
                        SaveConfirmPending::Quit => self.should_quit = true,
                    }
                }
                self.modal = None;
                return;
            }
            KeyCode::Char('n') if modal_kind == ModalKind::SaveConfirm => {
                if let Some(pending) = self.save_confirm_pending.take() {
                    match pending {
                        SaveConfirmPending::CloseTab(t) => {
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
                        SaveConfirmPending::Quit => self.should_quit = true,
                    }
                }
                self.modal = None;
                return;
            }
            KeyCode::Char(c) => {
                if let Some(modal) = self.modal.as_mut() {
                    modal.insert_char(c);
                }
                // Live search
                if modal_kind == ModalKind::Find {
                    let pattern = self.modal.as_ref().unwrap().input.clone();
                    let rope = &self.documents[tab].buffer.rope;
                    self.search.find(SearchConfig::case_insensitive(&pattern), rope);
                }
            }
            KeyCode::Backspace => {
                if let Some(modal) = self.modal.as_mut() {
                    modal.backspace();
                }
                if modal_kind == ModalKind::Find {
                    let pattern = self.modal.as_ref().unwrap().input.clone();
                    let rope = &self.documents[tab].buffer.rope;
                    self.search.find(SearchConfig::case_insensitive(&pattern), rope);
                }
            }
            KeyCode::Left => {
                if let Some(modal) = self.modal.as_mut() {
                    modal.cursor_left();
                }
            }
            KeyCode::Right => {
                if let Some(modal) = self.modal.as_mut() {
                    modal.cursor_right();
                }
            }
            _ => {}
        }
    }

    /// Execute a search across the current document.
    fn execute_search(&mut self, pattern: &str) {
        let tab = self.active_tab;
        let rope = &self.documents[tab].buffer.rope;
        self.search.find(SearchConfig::case_insensitive(pattern), rope);

        // Jump to first match
        if let Some(m) = self.search.current().cloned() {
            let doc = &mut self.documents[tab];
            let line = doc.buffer.char_to_line(m.start);
            let line_start = doc.buffer.line_to_char(line);
            let col = m.start - line_start;
            doc.cursor.goto(line, col, &doc.buffer);
            doc.ensure_cursor_visible(self.viewport_height, self.viewport_width);
        }

        let count = self.search.match_count();
        self.status_message = Some(if count > 0 {
            format!("{} match{}", count, if count == 1 { "" } else { "es" })
        } else {
            "No matches".to_string()
        });
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
        let ghost_text = if self.ai_pending && self.ghost_suggestion.is_none() {
            Some("...")
        } else {
            self.ghost_suggestion.as_deref()
        };
        let completion_dropdown = self.completion_list.as_ref().map(|c| (c.items.as_slice(), c.selected));
        frame.render_widget(
            EditorPane::new(doc, &self.theme, hl, &self.search)
                .ghost_text(ghost_text)
                .completion_dropdown(completion_dropdown),
            editor_area,
        );

        // Modal overlay
        if let Some(ref modal) = self.modal {
            let search_status = if modal.kind == ModalKind::Find {
                Some(self.search.status_text())
            } else {
                None
            };
            frame.render_widget(
                ModalWidget::new(modal, &self.theme).search_status(search_status),
                editor_area,
            );
        }

        // Status bar
        let search_status = if self.search.match_count() > 0 {
            Some(self.search.status_text())
        } else {
            None
        };
        let doc = &self.documents[self.active_tab];
        frame.render_widget(
            StatusBar::new(doc, &self.theme)
                .search_status(search_status)
                .message(self.status_message.clone()),
            main_chunks[2],
        );
    }
}
