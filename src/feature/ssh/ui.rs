use crate::config::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

pub struct SshDiffModalWidget<'a> {
    pub diff: &'a str,
    pub filename: &'a str,
    pub scroll: u16,
    pub theme: &'a Theme,
}

impl<'a> Widget for SshDiffModalWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 10 {
            return;
        }

        let modal_w = area.width.saturating_sub(10).min(100);
        let modal_h = area.height.saturating_sub(6).min(30);
        let x = area.x + (area.width - modal_w) / 2;
        let y = area.y + (area.height - modal_h) / 2;
        let diff_area = Rect::new(x, y, modal_w, modal_h);

        Clear.render(diff_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.ui.panel_border))
            .style(Style::default().bg(self.theme.editor.background))
            .title(format!(" SSH Sync Diff: {} ", self.filename))
            .title_bottom(" (y) Upload   (n) Cancel   (Up/Down) Scroll ")
            .title_alignment(Alignment::Center);

        let mut lines = Vec::new();
        for line in self.diff.lines() {
            let style = if line.starts_with('+') {
                Style::default().fg(self.theme.git.added)
            } else if line.starts_with('-') {
                Style::default().fg(self.theme.git.deleted)
            } else if line.starts_with("@@") {
                Style::default().fg(self.theme.syntax_color("keyword"))
            } else {
                Style::default().fg(self.theme.editor.foreground)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((self.scroll, 0));
        
        paragraph.render(diff_area, buf);
    }
}
