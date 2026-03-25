use std::io::stdout;
use std::path::Path;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use similar::{ChangeTag, TextDiff};

use super::models::{PatchEvent, Snapshot, TimelineOp};
use super::store::TimelineStore;

pub fn run_timeline_ui(file_path: &Path) -> anyhow::Result<()> {
    let store = TimelineStore::new(file_path);
    let patches = store.load_patches();
    let snapshots = store.load_snapshots();

    if patches.is_empty() {
        println!("No timeline history for {}", file_path.display());
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut list_state = ListState::default();
    list_state.select(Some(patches.len() - 1));

    let mut current_id = patches.last().unwrap().id;

    fn update_diff(
        id: u64,
        patches: &[PatchEvent],
        snapshots: &[Snapshot],
        store: &TimelineStore,
    ) -> Vec<Line<'static>> {
        let state_now = store.reconstruct_state(id, snapshots, patches);
        let state_prev = if id > 0 {
            store.reconstruct_state(id - 1, snapshots, patches)
        } else {
            "".to_string()
        };

        let diff = TextDiff::from_lines(&state_prev, &state_now);
        let mut lines = Vec::new();
        for change in diff.iter_all_changes() {
            let (color, prefix) = match change.tag() {
                ChangeTag::Delete => (Color::Red, "- "),
                ChangeTag::Insert => (Color::Green, "+ "),
                ChangeTag::Equal => (Color::DarkGray, "  "),
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(color)),
                Span::styled(
                    change.value().trim_end_matches('\n').to_string(),
                    Style::default().fg(color),
                ),
            ]));
        }
        lines
    }

    let mut current_diff_lines = update_diff(current_id, &patches, &snapshots, &store);

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(f.size());

            let items: Vec<ListItem> = patches
                .iter()
                .map(|p| {
                    let time_str = p.timestamp.format("%H:%M:%S").to_string();
                    let op_str = match &p.op {
                        TimelineOp::Insert { .. } => "Insert",
                        TimelineOp::Delete { .. } => "Delete",
                        TimelineOp::Replace { .. } => "Replace",
                    };
                    let content = format!("[{}] {} #{}", time_str, op_str, p.id);
                    ListItem::new(content)
                })
                .collect();

            let target_idx = list_state.selected().unwrap_or(0);
            let target_time = patches[target_idx]
                .timestamp
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Timeline"))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol(">> ");
            f.render_stateful_widget(list, chunks[0], &mut list_state);

            let diff_paragraph = Paragraph::new(current_diff_lines.clone()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Diff #{} ({}) | Press R to Restore", current_id, target_time)),
            );
            f.render_widget(diff_paragraph, chunks[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = match list_state.selected() {
                            Some(i) => {
                                if i >= patches.len() - 1 {
                                    i
                                } else {
                                    i + 1
                                }
                            }
                            None => 0,
                        };
                        list_state.select(Some(i));
                        current_id = patches[i].id;
                        current_diff_lines =
                            update_diff(current_id, &patches, &snapshots, &store);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = match list_state.selected() {
                            Some(i) => {
                                if i == 0 {
                                    0
                                } else {
                                    i - 1
                                }
                            }
                            None => 0,
                        };
                        list_state.select(Some(i));
                        current_id = patches[i].id;
                        current_diff_lines =
                            update_diff(current_id, &patches, &snapshots, &store);
                    }
                    KeyCode::Char('R') | KeyCode::Char('r') => {
                        let state = store.reconstruct_state(current_id, &snapshots, &patches);

                        if let Ok(content) = std::fs::read_to_string(file_path) {
                            let mut backup_path = file_path.to_path_buf();
                            backup_path.set_extension("bak.termedit");
                            let _ = std::fs::write(&backup_path, content);
                        }

                        let _ = std::fs::write(file_path, state);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
