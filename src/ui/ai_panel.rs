//! Full-editor overlay for Gemini chat (Ctrl/Cmd+K).

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};
use unicode_width::UnicodeWidthChar;

use crate::config::theme::Theme;
use crate::feature::gemini_chat::{preset_model_index, ChatRole, GeminiTurn, GEMINI_CHAT_MODELS};

#[derive(Debug, Clone)]
pub struct AiPanelState {
    pub visible: bool,
    pub turns: Vec<GeminiTurn>,
    pub input: String,
    /// First visible line in transcript when `!stick_transcript_to_bottom`.
    pub transcript_scroll: usize,
    /// When true, transcript stays pinned to the newest lines.
    pub stick_transcript_to_bottom: bool,
    /// Gemini `model_id` for API calls (may be any valid id, not only [`GEMINI_CHAT_MODELS`]).
    pub model_id: String,
    pub pending_req_id: Option<u64>,
    pub loading: bool,
    pub error: Option<String>,
    /// Title/footer spinner frame 0..4 when loading.
    pub spinner_frame: u8,
    /// Short pulse after send (visual emphasis).
    pub send_pulse: u8,
}

impl AiPanelState {
    pub fn new(model_id: String) -> Self {
        Self {
            visible: false,
            turns: vec![],
            input: String::new(),
            transcript_scroll: 0,
            stick_transcript_to_bottom: true,
            model_id: if model_id.is_empty() {
                GEMINI_CHAT_MODELS[0].to_string()
            } else {
                model_id
            },
            pending_req_id: None,
            loading: false,
            error: None,
            spinner_frame: 0,
            send_pulse: 0,
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.input.clear();
        self.transcript_scroll = 0;
        self.stick_transcript_to_bottom = true;
        self.pending_req_id = None;
        self.loading = false;
        self.error = None;
        self.send_pulse = 0;
    }

    pub fn open(&mut self) {
        self.visible = true;
        self.error = None;
        self.stick_transcript_to_bottom = true;
    }

    pub fn current_model_id(&self) -> &str {
        &self.model_id
    }

    pub fn cycle_model(&mut self, delta: isize) {
        let n = GEMINI_CHAT_MODELS.len();
        if n == 0 {
            return;
        }
        let pos = preset_model_index(self.model_id.as_str()).unwrap_or(0);
        let i = (pos as isize + delta).rem_euclid(n as isize) as usize;
        self.model_id = GEMINI_CHAT_MODELS[i].to_string();
    }

    /// Last assistant message text, if any.
    pub fn last_model_reply(&self) -> Option<&str> {
        for t in self.turns.iter().rev() {
            if matches!(t.role, ChatRole::Model) {
                return Some(t.text.as_str());
            }
        }
        None
    }

    fn prefix_for(role: &ChatRole) -> &'static str {
        match role {
            ChatRole::User => "You: ",
            ChatRole::Model => "AI: ",
        }
    }

    /// Wrapped lines for the transcript (prefix + body), for layout width `wrap_width`.
    pub fn build_transcript_lines(&self, wrap_width: usize) -> Vec<String> {
        let ww = wrap_width.max(8);
        let mut lines = Vec::new();
        for turn in &self.turns {
            let prefix = Self::prefix_for(&turn.role);
            for para in turn.text.split('\n') {
                let mut first = true;
                for chunk in wrap_line(para, ww.saturating_sub(prefix.len()), ww) {
                    if first {
                        lines.push(format!("{prefix}{chunk}"));
                        first = false;
                    } else {
                        let pad = " ".repeat(prefix.len().min(ww));
                        lines.push(format!("{pad}{chunk}"));
                    }
                }
                if para.is_empty() && first {
                    lines.push(prefix.trim_end().to_string());
                }
            }
        }
        lines
    }

    pub fn tick_spinner(&mut self) {
        if self.loading {
            self.spinner_frame = (self.spinner_frame + 1) % 4;
        }
        if self.send_pulse > 0 {
            self.send_pulse -= 1;
        }
    }

    /// Match panel inner transcript width for scroll math (see widget layout).
    pub fn inner_width_from_editor(editor_width: u16) -> usize {
        let w = (editor_width as usize).min(88).max(36);
        w.saturating_sub(4).max(8)
    }
}

fn wrap_line(input: &str, first_chunk_width: usize, subsequent_width: usize) -> Vec<String> {
    if input.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    let mut rest = input;
    let mut width_limit = first_chunk_width.max(1);
    while !rest.is_empty() {
        let (line, next_rest) = take_wrapped_width(rest, width_limit);
        out.push(line);
        rest = next_rest;
        width_limit = subsequent_width.max(1);
    }
    out
}

fn take_wrapped_width(s: &str, max_width: usize) -> (String, &str) {
    if s.is_empty() {
        return (String::new(), "");
    }
    let mut acc_width = 0usize;
    let mut end_byte = 0usize;
    for ch in s.chars() {
        let w = std::cmp::max(1, ch.width().unwrap_or(1));
        if acc_width + w > max_width && end_byte > 0 {
            break;
        }
        if acc_width + w > max_width && end_byte == 0 {
            end_byte += ch.len_utf8();
            break;
        }
        end_byte += ch.len_utf8();
        acc_width += w;
    }
    let chunk = &s[..end_byte];
    let rest = s[end_byte..].trim_start();
    (chunk.to_string(), rest)
}

pub struct AiPanelWidget<'a> {
    pub state: &'a AiPanelState,
    pub theme: &'a Theme,
}

impl<'a> Widget for AiPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.width < 24 || area.height < 8 || !self.state.visible {
            return;
        }

        let w = (area.width as usize).min(88).max(36) as u16;
        let h = (area.height.saturating_sub(2)).min(26).max(10);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;

        let bg = self.theme.editor.background;
        let fg = self.theme.editor.foreground;
        let border_c = if self.state.send_pulse > 0 {
            self.theme.ui.status_bar_bg
        } else {
            self.theme.ui.panel_border
        };
        let border = Style::default().fg(border_c).bg(bg);
        let title_st = Style::default()
            .fg(self.theme.ui.status_bar_bg)
            .bg(bg)
            .add_modifier(Modifier::BOLD);
        let user_hi = Style::default().fg(fg).bg(self.theme.editor.gutter_bg);
        let ai_hi = Style::default()
            .fg(fg)
            .bg(self.theme.ui.ai_response_bg);
        let dim = Style::default().fg(self.theme.editor.line_number).bg(bg);
        let input_st = Style::default().fg(fg).bg(self.theme.ui.find_bar_bg);

        for row in 0..h {
            for col in 0..w {
                buf.set_string(x + col, y + row, " ", Style::default().bg(bg));
            }
        }

        let spin = if self.state.loading {
            ['|', '/', '-', '\\'][self.state.spinner_frame as usize % 4]
        } else {
            ' '
        };
        let model = self.state.current_model_id();
        let title = format!(" AI assistant {}  [{}]", spin, model);
        let title_trim = truncate_str(&title, w as usize - 2);
        buf.set_string(x + 1, y, &title_trim, title_st);

        let inner_w = (w as usize).saturating_sub(4).max(8);
        let footer_h = 2u16;
        let transcript_h = h.saturating_sub(3 + footer_h);

        let lines = self.state.build_transcript_lines(inner_w);
        let vis = transcript_h as usize;
        let max_scroll = lines.len().saturating_sub(vis);
        let scroll = if self.state.stick_transcript_to_bottom {
            max_scroll
        } else {
            self.state.transcript_scroll.min(max_scroll)
        };
        for row in 0..transcript_h {
            let line_idx = scroll + row as usize;
            let text = lines.get(line_idx).map(String::as_str).unwrap_or("");
            let st = if text.starts_with("You:") {
                user_hi
            } else if text.starts_with("AI:") {
                ai_hi
            } else {
                Style::default().fg(fg).bg(bg)
            };
            let trimmed = truncate_str(text, inner_w + 4);
            buf.set_string(x + 2, y + 1 + row, &trimmed, st);
        }

        let input_row = y + 1 + transcript_h;
        let prompt = if self.state.input.is_empty() {
            "> (Enter send · Shift+Enter newline)".to_string()
        } else {
            format!("> {}", self.state.input)
        };
        let inp_show = truncate_str(&prompt, inner_w + 4);
        buf.set_string(x + 2, input_row, &inp_show, input_st);

        let err_row = input_row + 1;
        if let Some(ref e) = self.state.error {
            let e_show = truncate_str(e, inner_w + 4);
            buf.set_string(
                x + 2,
                err_row,
                &e_show,
                Style::default().fg(self.theme.git.deleted).bg(bg),
            );
        } else if self.state.loading {
            buf.set_string(x + 2, err_row, " Waiting for model…", dim);
        } else {
            buf.set_string(x + 2, err_row, " Ctrl+Shift+I insert last reply", dim);
        }

        let foot = "Esc · Tab model · Pg↑↓ · Ctrl+K panel · Ctrl+Shift+I insert · Ctrl+Shift+U ideas";
        buf.set_string(
            x + 1,
            y + h - 1,
            truncate_str(foot, w as usize - 2).as_str(),
            border,
        );
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    format!("{}…", &s[..max_chars.saturating_sub(1).min(s.len())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_preserves_empty_paragraph() {
        let mut st = AiPanelState::new(GEMINI_CHAT_MODELS[0].to_string());
        st.turns.push(GeminiTurn {
            role: ChatRole::User,
            text: "hi".into(),
        });
        let lines = st.build_transcript_lines(20);
        assert!(lines.iter().any(|l| l.contains("You:")));
    }
}
