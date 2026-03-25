//! TUI for the Smart Log Explorer.
//!
//! Full-screen ratatui layout:
//!
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Smart Log Explorer  /var/log/syslog  [TAIL]                │  header
//! ├─────────────────────────────────────────────────────────────┤
//! │  14:02:01  ERROR  api  connection refused (502ms)   ⚡      │  log list
//! │  14:02:00  INFO   db   query ok                             │
//! │  …                                                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Query ▶  /error last 10 min_                               │  query bar
//! ├─────────────────────────────────────────────────────────────┤
//! │  1234 lines  · 12 ERR  · 3 WARN  · p95 203ms  · ⚡ ANOMALY │  status
//! └─────────────────────────────────────────────────────────────┘
//!
//! Key bindings:
//!   /            → query mode (slash commands)
//!   >            → AI query mode
//!   j/k, ↑↓      → scroll
//!   g/G          → top / bottom
//!   PgUp/PgDn    → page scroll
//!   Enter        → run query
//!   Esc          → clear query / close AI panel
//!   t            → toggle tail mode
//!   q / Ctrl-C   → quit

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::Terminal;
use tokio::sync::mpsc;

use super::aggregation::{compute_stats, group_by, LogStats};
use super::ai::{AiReport};
use super::anomaly::AnomalyConfig;
use super::config::LogsConfig;
use super::parser::{LogEntry, LogLevel};
use super::query::{filter_indices, Query, GroupField};
use super::streamer::StreamControl;

// ─── Palette ─────────────────────────────────────────────────────────────────

const COL_BG:       Color = Color::Rgb(18, 18, 28);
const COL_HEADER:   Color = Color::Rgb(26, 29, 48);
const COL_BORDER:   Color = Color::Rgb(59, 66, 97);
const COL_ERROR:    Color = Color::Rgb(247, 118, 142);
const COL_WARN:     Color = Color::Rgb(255, 199, 119);
const COL_INFO:     Color = Color::Rgb(122, 162, 247);
const COL_DEBUG:    Color = Color::Rgb(86, 95, 137);
const COL_TRACE:    Color = Color::Rgb(68, 75, 106);
const COL_FG:       Color = Color::Rgb(192, 202, 245);
const COL_MUTED:    Color = Color::Rgb(130, 140, 170);
const COL_ANOMALY:  Color = Color::Rgb(187, 154, 247);
const COL_STATUS_BG:Color = Color::Rgb(36, 40, 59);
const COL_QUERY_BG: Color = Color::Rgb(26, 29, 50);
const COL_CURSOR:   Color = Color::Rgb(158, 206, 106);
const COL_SERVICE:  Color = Color::Rgb(42, 195, 222);
const COL_TIME:     Color = Color::Rgb(115, 218, 202);
const COL_AI_BG:    Color = Color::Rgb(22, 30, 46);
const COL_AI_TITLE: Color = Color::Rgb(187, 154, 247);

fn level_color(level: &LogLevel) -> Color {
    match level {
        LogLevel::Error   => COL_ERROR,
        LogLevel::Warn    => COL_WARN,
        LogLevel::Info    => COL_INFO,
        LogLevel::Debug   => COL_DEBUG,
        LogLevel::Trace   => COL_TRACE,
        LogLevel::Unknown => COL_MUTED,
    }
}

// ─── App state ────────────────────────────────────────────────────────────────

enum InputMode {
    Normal,
    Query,
    AiQuery,
}

pub struct LogExplorerState {
    /// All parsed entries (capped at `max_lines`).
    pub entries: Vec<LogEntry>,
    /// Indices into `entries` that pass the current query.
    pub visible: Vec<usize>,
    /// Scroll offset into `visible`.
    pub scroll:  usize,
    /// ratatui list state (for cursor highlight).
    pub list_state: ListState,
    /// Current query string being typed.
    pub query_input: String,
    /// Active compiled query.
    pub active_query: Query,
    /// Whether we are in tail mode.
    pub tail:    bool,
    /// Input mode.
    pub mode:    InputMode,
    /// Latest AI report.
    pub ai_report: Option<AiReport>,
    /// AI panel scroll offset.
    pub ai_scroll: usize,
    /// Whether the AI request is in flight.
    pub ai_loading: bool,
    /// Computed stats for the current visible set.
    pub stats: LogStats,
    /// Group-by data when the query is GroupBy.
    pub group_data: Option<BTreeMap<String, usize>>,
    /// File path label.
    pub file_label: String,
    /// Anomaly config.
    pub anomaly_cfg: AnomalyConfig,
    /// Maximum entries to keep in memory.
    pub max_lines: usize,
}

impl LogExplorerState {
    pub fn new(file_path: &PathBuf, cfg: &LogsConfig) -> Self {
        let file_label = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let mut s = LogExplorerState {
            entries:     Vec::new(),
            visible:     Vec::new(),
            scroll:      0,
            list_state:  ListState::default(),
            query_input: String::new(),
            active_query: Query::None,
            tail:        true,
            mode:        InputMode::Normal,
            ai_report:   None,
            ai_scroll:   0,
            ai_loading:  false,
            stats:       LogStats::default(),
            group_data:  None,
            file_label,
            anomaly_cfg: cfg.anomaly_config(),
            max_lines:   cfg.max_lines,
        };
        s.rebuild_visible();
        s
    }

    /// Ingest a batch of new entries, run anomaly detection, rebuild visible set.
    pub fn ingest(&mut self, mut batch: Vec<LogEntry>) {
        // Run anomaly detection on the batch before appending.
        super::anomaly::run_all(&mut batch, &self.anomaly_cfg);

        self.entries.extend(batch);

        // Enforce ring-buffer cap (drop oldest).
        if self.entries.len() > self.max_lines {
            let excess = self.entries.len() - self.max_lines;
            self.entries.drain(0..excess);
            // Re-index line numbers isn't necessary — they're baked into LogEntry.raw
        }

        self.rebuild_visible();

        // Auto-scroll to bottom in tail mode.
        if self.tail && !self.visible.is_empty() {
            self.scroll = self.visible.len().saturating_sub(1);
        }
    }

    pub fn rebuild_visible(&mut self) {
        self.visible = filter_indices(&self.entries, &self.active_query);

        // Group-by view
        if let Query::GroupBy(ref field) = self.active_query {
            let iter = self.visible.iter().map(|&i| &self.entries[i]);
            self.group_data = Some(group_by(field, iter));
        } else {
            self.group_data = None;
        }

        // Stats over visible set
        let visible_entries: Vec<&LogEntry> = self.visible.iter().map(|&i| &self.entries[i]).collect();
        // We need an owned slice for compute_stats
        let owned: Vec<LogEntry> = visible_entries.into_iter().cloned().collect();
        self.stats = compute_stats(&owned);

        // Clamp scroll
        if !self.visible.is_empty() {
            self.scroll = self.scroll.min(self.visible.len() - 1);
        } else {
            self.scroll = 0;
        }
    }

    pub fn scroll_down(&mut self, n: usize) {
        if self.visible.is_empty() { return; }
        self.scroll = (self.scroll + n).min(self.visible.len() - 1);
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_top(&mut self) { self.scroll = 0; }

    pub fn scroll_bottom(&mut self) {
        if !self.visible.is_empty() {
            self.scroll = self.visible.len() - 1;
        }
    }
}

// ─── Rendering ────────────────────────────────────────────────────────────────

/// Render the log explorer to the terminal.
pub fn render(f: &mut ratatui::Frame, state: &mut LogExplorerState) {
    let area = f.area();

    // Background
    f.render_widget(
        Block::default().style(Style::default().bg(COL_BG)),
        area,
    );

    // Decide layout depending on AI panel.
    let has_ai = state.ai_report.is_some() || state.ai_loading;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_ai {
            vec![
                Constraint::Length(1), // header
                Constraint::Min(6),    // log list
                Constraint::Length(3), // query bar
                Constraint::Length(1), // status bar
                Constraint::Length(12), // AI panel
            ]
        } else {
            vec![
                Constraint::Length(1), // header
                Constraint::Min(6),    // log list
                Constraint::Length(3), // query bar
                Constraint::Length(1), // status bar
            ]
        })
        .split(area);

    render_header(f, chunks[0], state);
    render_log_list(f, chunks[1], state);
    render_query_bar(f, chunks[2], state);
    render_status_bar(f, chunks[3], state);

    if has_ai {
        render_ai_panel(f, chunks[4], state);
    }
}

fn render_header(f: &mut ratatui::Frame, area: Rect, state: &LogExplorerState) {
    let tail_label = if state.tail { " [TAIL] " } else { " [PAUSED] " };
    let title = format!(
        " ⚡ Smart Log Explorer  ─  {}{}",
        state.file_label, tail_label
    );
    let para = Paragraph::new(title)
        .style(Style::default().fg(COL_FG).bg(COL_HEADER).add_modifier(Modifier::BOLD));
    f.render_widget(para, area);
}

fn render_log_list(f: &mut ratatui::Frame, area: Rect, state: &mut LogExplorerState) {
    // Group-by mode
    if let Some(ref groups) = state.group_data {
        let items: Vec<ListItem> = groups.iter().map(|(key, count)| {
            let line = Line::from(vec![
                Span::styled(format!(" {:>6} ", count), Style::default().fg(COL_WARN).add_modifier(Modifier::BOLD)),
                Span::styled("  ", Style::default()),
                Span::styled(key.clone(), Style::default().fg(COL_INFO)),
            ]);
            ListItem::new(line)
        }).collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(COL_BORDER))
                .title(" Group Summary "))
            .style(Style::default().bg(COL_BG).fg(COL_FG))
            .highlight_style(Style::default().bg(Color::Rgb(40, 48, 72)));

        f.render_stateful_widget(list, area, &mut state.list_state);
        return;
    }

    let height = area.height as usize;
    let start = state.scroll.saturating_sub(height / 2);
    let view_indices: Vec<usize> = state.visible.iter()
        .copied()
        .skip(start)
        .take(height + 4)
        .collect();

    let items: Vec<ListItem> = view_indices.iter().map(|&i| {
        let e = &state.entries[i];
        let lc = level_color(&e.level);

        // Timestamp portion
        #[cfg(feature = "logs")]
        let ts_str = e.timestamp
            .map(|ts| ts.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "──:──:──".to_string());
        #[cfg(not(feature = "logs"))]
        let ts_str = "──:──:──".to_string();

        // Response time badge
        let rt_badge = e.response_time.map(|rt| {
            if rt >= 1000.0 {
                format!(" {:.1}s ", rt / 1000.0)
            } else {
                format!(" {:.0}ms ", rt)
            }
        }).unwrap_or_default();

        let anomaly_badge = if e.anomaly { " ⚡" } else { "" };

        let svc = e.service.as_deref().unwrap_or("─");

        let mut spans = vec![
            Span::styled(format!(" {} ", ts_str), Style::default().fg(COL_TIME)),
            Span::styled(format!(" {} ", e.level.label()), Style::default()
                .fg(lc).add_modifier(if matches!(e.level, LogLevel::Error) { Modifier::BOLD } else { Modifier::empty() })),
            Span::styled(" ", Style::default()),
            Span::styled(format!("{:<12}", svc), Style::default().fg(COL_SERVICE)),
            Span::styled(" ", Style::default()),
            Span::styled(e.message.as_str(), Style::default().fg(if e.anomaly { COL_ANOMALY } else { COL_FG })),
        ];
        if !rt_badge.is_empty() {
            spans.push(Span::styled(rt_badge, Style::default().fg(COL_MUTED)));
        }
        if !anomaly_badge.is_empty() {
            spans.push(Span::styled(anomaly_badge, Style::default().fg(COL_ANOMALY).add_modifier(Modifier::BOLD)));
        }

        let bg = if e.anomaly { Color::Rgb(30, 22, 46) } else { COL_BG };
        let line = Line::from(spans).style(Style::default().bg(bg));
        ListItem::new(line)
    }).collect();

    // Adjust list_state to reflect scroll position
    let relative_pos = state.scroll.saturating_sub(start);
    state.list_state.select(Some(relative_pos.min(items.len().saturating_sub(1))));

    let title = if state.visible.is_empty() {
        " No entries match the current filter ".to_string()
    } else {
        format!(" {} / {} entries ", state.visible.len(), state.entries.len())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(COL_BORDER))
            .title(title))
        .style(Style::default().bg(COL_BG).fg(COL_FG))
        .highlight_style(Style::default().bg(Color::Rgb(36, 44, 66)).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, area, &mut state.list_state);
}

fn render_query_bar(f: &mut ratatui::Frame, area: Rect, state: &LogExplorerState) {
    let (prefix, prefix_color) = match state.mode {
        InputMode::Query     => ("Query ▶  ", COL_INFO),
        InputMode::AiQuery   => ("AI ✦    ", COL_AI_TITLE),
        InputMode::Normal if !state.query_input.is_empty() => ("Filter   ", COL_WARN),
        _ => ("Press /  ", COL_MUTED),
    };

    let cursor = if matches!(state.mode, InputMode::Query | InputMode::AiQuery) { "█" } else { "" };

    let line = Line::from(vec![
        Span::styled(format!(" {} ", prefix), Style::default().fg(prefix_color).add_modifier(Modifier::BOLD)),
        Span::styled(state.query_input.as_str(), Style::default().fg(COL_FG)),
        Span::styled(cursor, Style::default().fg(COL_CURSOR).add_modifier(Modifier::SLOW_BLINK)),
    ]);

    let help_line = Line::from(vec![
        Span::styled("  j/k scroll  t tail  g/G top/bot  /query  >ai  q quit",
            Style::default().fg(COL_MUTED)),
    ]);

    let para = Paragraph::new(vec![line, help_line])
        .block(Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(COL_BORDER))
            .title(" Query "))
        .style(Style::default().bg(COL_QUERY_BG));
    f.render_widget(para, area);
}

fn render_status_bar(f: &mut ratatui::Frame, area: Rect, state: &LogExplorerState) {
    let s = &state.stats;
    let mut parts: Vec<Span> = vec![
        Span::raw(" "),
        Span::styled(format!("{} lines", s.total), Style::default().fg(COL_FG)),
        Span::styled("  ·  ", Style::default().fg(COL_MUTED)),
        Span::styled(format!("{} ERR", s.errors), Style::default().fg(COL_ERROR).add_modifier(Modifier::BOLD)),
        Span::styled("  ·  ", Style::default().fg(COL_MUTED)),
        Span::styled(format!("{} WARN", s.warns), Style::default().fg(COL_WARN)),
    ];

    if let Some(p95) = s.p95_response_time {
        parts.push(Span::styled("  ·  ", Style::default().fg(COL_MUTED)));
        parts.push(Span::styled(
            format!("p95 {:.0}ms", p95),
            Style::default().fg(COL_INFO),
        ));
    }

    if s.anomalies > 0 {
        parts.push(Span::styled("  ·  ", Style::default().fg(COL_MUTED)));
        parts.push(Span::styled(
            format!("⚡ {} ANOMAL{}", s.anomalies, if s.anomalies == 1 { "Y" } else { "IES" }),
            Style::default().fg(COL_ANOMALY).add_modifier(Modifier::BOLD),
        ));
    }

    let line = Line::from(parts);
    let para = Paragraph::new(line)
        .style(Style::default().bg(COL_STATUS_BG).fg(COL_FG));
    f.render_widget(para, area);
}

fn render_ai_panel(f: &mut ratatui::Frame, area: Rect, state: &LogExplorerState) {
    let content = if state.ai_loading {
        "  Thinking… (Gemini is analyzing your logs)".to_string()
    } else if let Some(ref r) = state.ai_report {
        r.to_display()
    } else {
        String::new()
    };

    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(COL_AI_TITLE))
            .title(Span::styled(" ✦ AI Analysis (Esc to close) ", Style::default().fg(COL_AI_TITLE).add_modifier(Modifier::BOLD))))
        .style(Style::default().bg(COL_AI_BG).fg(COL_FG))
        .wrap(Wrap { trim: false })
        .scroll((state.ai_scroll as u16, 0));
    f.render_widget(para, area);
}

// ─── Main event loop ──────────────────────────────────────────────────────────

/// Run the interactive TUI.
pub fn run_tui(
    file_path: PathBuf,
    from_start: bool,
    api_key: Option<String>,
    model_id: String,
    cfg: LogsConfig,
) -> anyhow::Result<()> {
    // Set up terminal
    let stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::BufWriter::new(stdout);
    use std::io::Write;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    // Start tokio runtime for streamer + AI.
    let rt = super::streamer::build_runtime()?;

    let (ctrl_tx, mut entries_rx) = {
        let _guard = rt.enter();
        super::streamer::spawn(file_path.clone(), from_start, cfg.clone())
    };

    let mut state = LogExplorerState::new(&file_path, &cfg);

    // AI channel: (query, snippet) → Option<AiReport>
    let (ai_req_tx, mut ai_req_rx) = tokio::sync::mpsc::channel::<(String, String)>(4);
    let (ai_res_tx, mut ai_res_rx) = tokio::sync::mpsc::channel::<Result<AiReport, String>>(4);
    let api_key_clone = api_key.clone();
    let model_clone = model_id.clone();
    rt.spawn(async move {
        while let Some((query, snippet)) = ai_req_rx.recv().await {
            let result = if let Some(ref key) = api_key_clone {
                super::ai::query_async(query, snippet, key.clone(), model_clone.clone())
                    .await
                    .map_err(|e| e.to_string())
            } else {
                Err("No GEMINI_API_KEY set. Export GEMINI_API_KEY or pass --gemini-api-key".to_string())
            };
            let _ = ai_res_tx.send(result).await;
        }
    });

    let tick = Duration::from_millis(80);
    let mut last_tick = Instant::now();

    loop {
        // Drain new log batches (non-blocking).
        while let Ok(batch) = entries_rx.try_recv() {
            state.ingest(batch);
        }

        // Drain pending AI responses.
        while let Ok(result) = ai_res_rx.try_recv() {
            state.ai_loading = false;
            match result {
                Ok(report) => state.ai_report = Some(report),
                Err(e)     => {
                    state.ai_report = Some(AiReport {
                        summary:     format!("Error: {}", e),
                        root_cause:  String::new(),
                        suggestions: vec![],
                        raw:         String::new(),
                    });
                }
            }
        }

        terminal.draw(|f| render(f, &mut state))?;

        // Event handling with timeout.
        let timeout = tick.checked_sub(last_tick.elapsed()).unwrap_or_default();
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if !handle_key(key, &mut state, &ctrl_tx, &ai_req_tx, &rt) {
                    break; // quit
                }
            }
        }

        if last_tick.elapsed() >= tick {
            last_tick = Instant::now();
        }
    }

    // Cleanup
    let _ = rt.block_on(ctrl_tx.send(StreamControl::Stop));
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
    )?;
    crossterm::terminal::disable_raw_mode()?;
    terminal.show_cursor()?;
    Ok(())
}

// Returns false when the user wants to quit.
fn handle_key(
    key:   crossterm::event::KeyEvent,
    state: &mut LogExplorerState,
    ctrl_tx:    &mpsc::Sender<StreamControl>,
    ai_req_tx:  &mpsc::Sender<(String, String)>,
    rt:         &tokio::runtime::Runtime,
) -> bool {
    use crossterm::event::KeyEventKind;
    if key.kind == KeyEventKind::Release { return true; }

    match &state.mode {
        InputMode::Normal => handle_normal(key, state, ctrl_tx, rt),
        InputMode::Query | InputMode::AiQuery => handle_input(key, state, ai_req_tx, rt),
    }
}

fn handle_normal(
    key:    crossterm::event::KeyEvent,
    state:  &mut LogExplorerState,
    ctrl_tx: &mpsc::Sender<StreamControl>,
    rt:      &tokio::runtime::Runtime,
) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return false,
        KeyCode::Char('c') if ctrl             => return false,

        // Scroll
        KeyCode::Char('j') | KeyCode::Down     => { state.tail = false; state.scroll_down(1); }
        KeyCode::Char('k') | KeyCode::Up       => { state.tail = false; state.scroll_up(1); }
        KeyCode::Char('d') if ctrl             => { state.tail = false; state.scroll_down(10); }
        KeyCode::Char('u') if ctrl             => { state.tail = false; state.scroll_up(10); }
        KeyCode::PageDown                      => { state.tail = false; state.scroll_down(20); }
        KeyCode::PageUp                        => { state.tail = false; state.scroll_up(20); }
        KeyCode::Char('g')                     => { state.tail = false; state.scroll_top(); }
        KeyCode::Char('G')                     => { state.tail = true;  state.scroll_bottom(); }

        // Tail toggle
        KeyCode::Char('t') | KeyCode::Char('T') => {
            state.tail = !state.tail;
            let msg = if state.tail { StreamControl::Resume } else { StreamControl::Pause };
            let _ = rt.block_on(ctrl_tx.send(msg));
            if state.tail { state.scroll_bottom(); }
        }

        // Enter query mode
        KeyCode::Char('/') => {
            state.mode = InputMode::Query;
            if state.query_input.is_empty() {
                state.query_input.push('/');
            }
        }
        KeyCode::Char('>') => {
            state.mode = InputMode::AiQuery;
            if state.query_input.is_empty() {
                state.query_input.push('>');
            }
        }

        // Clear query
        KeyCode::Esc => {
            state.query_input.clear();
            state.active_query = Query::None;
            state.ai_report = None;
            state.rebuild_visible();
        }

        // AI panel scroll
        KeyCode::Char(']') => { state.ai_scroll += 1; }
        KeyCode::Char('[') => { state.ai_scroll = state.ai_scroll.saturating_sub(1); }

        _ => {}
    }
    true
}

fn handle_input(
    key:       crossterm::event::KeyEvent,
    state:     &mut LogExplorerState,
    ai_req_tx: &mpsc::Sender<(String, String)>,
    rt:        &tokio::runtime::Runtime,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            state.mode = InputMode::Normal;
            state.query_input.clear();
            state.active_query = Query::None;
            state.ai_report = None;
            state.rebuild_visible();
        }
        KeyCode::Enter => {
            let input = state.query_input.trim().to_string();
            let q = Query::parse(&input).unwrap_or(Query::None);

            if let Query::Ai(ref nl) = q {
                // Issue AI request
                let snippet = collect_snippet(&state.entries, &state.visible, 200);
                let nl = nl.clone();
                state.ai_loading = true;
                state.ai_report = None;
                let _ = rt.block_on(ai_req_tx.send((nl, snippet)));
            }

            state.active_query = q;
            state.rebuild_visible();
            state.mode = InputMode::Normal;

            if state.tail { state.scroll_bottom(); }
        }
        KeyCode::Backspace => {
            state.query_input.pop();
        }
        KeyCode::Char(c) => {
            state.query_input.push(c);
        }
        _ => {}
    }
    true
}

/// Collect the most recent `max_lines` visible log lines as a text snippet for AI.
fn collect_snippet(entries: &[LogEntry], visible: &[usize], max_lines: usize) -> String {
    visible.iter().rev().take(max_lines)
        .rev()
        .map(|&i| entries[i].raw.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
