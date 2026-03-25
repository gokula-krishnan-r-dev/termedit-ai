/// Main editor pane — renders the text buffer with line numbers,
/// syntax highlighting, cursor, and selection.

use ratatui::{
    buffer::Buffer as RatBuffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::Widget,
};

use crate::config::theme::Theme;
use crate::core::document::Document;
use crate::feature::search::Search;
use crate::feature::syntax::SyntaxHighlighter;

/// The editor pane widget.
pub struct EditorPane<'a> {
    doc: &'a Document,
    theme: &'a Theme,
    highlighter: &'a SyntaxHighlighter,
    search: &'a Search,
    gutter_width: u16,
    focused: bool,
    /// Inline suggestion (ghost text) to show after cursor on current line.
    ghost_text: Option<&'a str>,
    /// Completion dropdown: (items, selected index). Rendered below cursor line.
    completion_dropdown: Option<(&'a [String], usize)>,
    /// When true, reserve the rightmost column for search match tick marks.
    show_match_strip: bool,
    /// Absolute character indices of the bracket pair highlighting the cursor (if any).
    bracket_highlight: Option<(usize, usize)>,
}

impl<'a> EditorPane<'a> {
    /// Create a new editor pane.
    pub fn new(
        doc: &'a Document,
        theme: &'a Theme,
        highlighter: &'a SyntaxHighlighter,
        search: &'a Search,
    ) -> Self {
        let line_count = doc.buffer.line_count();
        let gutter_width = format!("{}", line_count).len().max(3) as u16 + 2; // padding

        Self {
            doc,
            theme,
            highlighter,
            search,
            gutter_width,
            focused: true,
            ghost_text: None,
            completion_dropdown: None,
            show_match_strip: false,
            bracket_highlight: None,
        }
    }

    /// Highlight the bracket pair under the cursor (`()`, `[]`, `{}`).
    pub fn bracket_highlight(mut self, pair: Option<(usize, usize)>) -> Self {
        self.bracket_highlight = pair;
        self
    }

    /// Show a one-column overview of match lines on the right edge.
    pub fn match_strip(mut self, on: bool) -> Self {
        self.show_match_strip = on;
        self
    }

    /// Set completion dropdown (list of items, selected index). Shown below cursor line.
    pub fn completion_dropdown(mut self, items: Option<(&'a [String], usize)>) -> Self {
        self.completion_dropdown = items;
        self
    }

    /// Set ghost text (inline suggestion) to show after the cursor.
    pub fn ghost_text(mut self, ghost_text: Option<&'a str>) -> Self {
        self.ghost_text = ghost_text;
        self
    }

    /// Set whether this pane is focused.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Get the gutter width for layout calculations.
    pub fn gutter_width(&self) -> u16 {
        self.gutter_width
    }

    /// Build a styled line with syntax highlighting and search matches.
    fn render_line(
        &self,
        line_idx: usize,
        viewport_width: usize,
    ) -> Vec<Span<'static>> {
        let text = self.doc.buffer.line_text(line_idx);
        let scroll_x = self.doc.scroll_x;

        // Get syntax highlights for this line
        let highlight_spans = self.highlighter.highlight_line(&text);

        // Build character-level color map
        let chars: Vec<char> = text.chars().collect();
        let mut colors: Vec<Color> = vec![self.theme.editor.foreground; chars.len()];

        // Apply syntax colors
        for span in &highlight_spans {
            let color = self.theme.syntax_color(&span.highlight);
            // Convert byte offsets to char offsets
            let char_start = text[..span.start.min(text.len())].chars().count();
            let char_end = text[..span.end.min(text.len())].chars().count();
            for i in char_start..char_end.min(chars.len()) {
                colors[i] = color;
            }
        }

        // Check for selection
        let selection_range = self.doc.cursor.selection_range(&self.doc.buffer);

        // Check for search matches
        let search_matches = &self.search.matches;
        let current_match_idx = self.search.current_match;

        // Build spans, applying scroll offset
        let mut spans = Vec::new();
        let visible_start = scroll_x;
        let visible_end = (scroll_x + viewport_width).min(chars.len());

        if visible_start >= chars.len() {
            return vec![Span::raw("")];
        }

        let line_char_start = self.doc.buffer.line_to_char(line_idx);
        let in_sel = |ac: usize| {
            selection_range.map_or(false, |(s, e)| ac >= s && ac < e)
        };

        let mut i = visible_start;
        while i < visible_end {
            let abs_char = line_char_start + i;
            let fg = colors.get(i).copied().unwrap_or(self.theme.editor.foreground);
            let mut bg = if line_idx == self.doc.cursor.line {
                self.theme.editor.current_line_bg
            } else {
                self.theme.editor.background
            };

            // Check if in selection
            if in_sel(abs_char) {
                bg = self.theme.editor.selection_bg;
            }

            // Bracket pair (skip when inside selection; before search)
            if let Some((a, b)) = self.bracket_highlight {
                if !in_sel(abs_char) && (abs_char == a || abs_char == b) {
                    bg = self.theme.ui.bracket_match_bg;
                }
            }

            // Check if in search match
            for (match_idx, m) in search_matches.iter().enumerate() {
                if abs_char >= m.start && abs_char < m.end {
                    bg = if current_match_idx == Some(match_idx) {
                        self.theme.ui.search_current_bg
                    } else {
                        self.theme.ui.search_match_bg
                    };
                }
            }

            // Find how many consecutive chars share the same styling
            let mut j = i + 1;
            while j < visible_end {
                let next_abs = line_char_start + j;
                let next_fg = colors.get(j).copied().unwrap_or(self.theme.editor.foreground);
                let mut next_bg = if line_idx == self.doc.cursor.line {
                    self.theme.editor.current_line_bg
                } else {
                    self.theme.editor.background
                };

                if in_sel(next_abs) {
                    next_bg = self.theme.editor.selection_bg;
                }

                if let Some((ba, bb)) = self.bracket_highlight {
                    if !in_sel(next_abs) && (next_abs == ba || next_abs == bb) {
                        next_bg = self.theme.ui.bracket_match_bg;
                    }
                }

                for (match_idx, m) in search_matches.iter().enumerate() {
                    if next_abs >= m.start && next_abs < m.end {
                        next_bg = if current_match_idx == Some(match_idx) {
                            self.theme.ui.search_current_bg
                        } else {
                            self.theme.ui.search_match_bg
                        };
                    }
                }

                if next_fg != fg || next_bg != bg {
                    break;
                }
                j += 1;
            }

            let segment: String = chars[i..j].iter().collect();
            spans.push(Span::styled(
                segment,
                Style::default().fg(fg).bg(bg),
            ));
            i = j;
        }

        // Fill remaining viewport width with background
        if visible_end - visible_start < viewport_width {
            let remaining = viewport_width - (visible_end - visible_start);
            let bg = if line_idx == self.doc.cursor.line {
                self.theme.editor.current_line_bg
            } else {
                self.theme.editor.background
            };
            spans.push(Span::styled(
                " ".repeat(remaining),
                Style::default().bg(bg),
            ));
        }

        spans
    }
}

impl<'a> Widget for EditorPane<'a> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let viewport_height = area.height as usize;
        let show_strip = self.show_match_strip
            && !self.search.match_lines.is_empty()
            && area.width > self.gutter_width + 2;
        let strip_cols = u16::from(show_strip);
        let content_width = (area.width - self.gutter_width - strip_cols).max(1) as usize;
        let content_right = area.x + self.gutter_width + content_width as u16;
        let strip_x = area.x + area.width.saturating_sub(1);
        let cur_line = self.search.current_match_line(&self.doc.buffer.rope);

        for row in 0..viewport_height {
            let line_idx = self.doc.scroll_y + row;
            let y = area.y + row as u16;

            if line_idx < self.doc.buffer.line_count() {
                // Render gutter (line number)
                let line_num = format!(
                    "{:>width$} ",
                    line_idx + 1,
                    width = (self.gutter_width - 1) as usize
                );
                let gutter_style = if line_idx == self.doc.cursor.line {
                    Style::default()
                        .fg(self.theme.editor.line_number_active)
                        .bg(self.theme.editor.gutter_bg)
                } else {
                    Style::default()
                        .fg(self.theme.editor.line_number)
                        .bg(self.theme.editor.gutter_bg)
                };

                for (i, ch) in line_num.chars().enumerate() {
                    let x = area.x + i as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, ch.to_string(), gutter_style);
                    }
                }

                // Render text content
                let spans = self.render_line(line_idx, content_width);
                let text_x = area.x + self.gutter_width;
                let mut col = 0u16;
                for span in &spans {
                    for ch in span.content.chars() {
                        let x = text_x + col;
                        if x < content_right {
                            buf.set_string(x, y, ch.to_string(), span.style);
                        }
                        col += 1;
                    }
                }

                if show_strip {
                    let st = if cur_line == Some(line_idx) {
                        Style::default()
                            .fg(self.theme.editor.foreground)
                            .bg(self.theme.ui.search_current_bg)
                    } else if self.search.match_lines.contains(&line_idx) {
                        Style::default()
                            .fg(self.theme.editor.foreground)
                            .bg(self.theme.ui.search_match_bg)
                    } else {
                        Style::default()
                            .fg(self.theme.ui.find_toggle_off_fg)
                            .bg(self.theme.editor.background)
                    };
                    buf.set_string(strip_x, y, "▌", st);
                }
            } else {
                // Empty line below document — show tilde
                let tilde_style = Style::default()
                    .fg(self.theme.editor.line_number)
                    .bg(self.theme.editor.background);
                let padding = " ".repeat((self.gutter_width - 2) as usize);
                buf.set_string(area.x, y, format!("{}~ ", padding), tilde_style);

                // Fill rest with background
                let bg_style = Style::default().bg(self.theme.editor.background);
                for x in (area.x + self.gutter_width)..content_right {
                    buf.set_string(x, y, " ", bg_style);
                }
                if show_strip {
                    buf.set_string(
                        strip_x,
                        y,
                        " ",
                        Style::default().bg(self.theme.editor.background),
                    );
                }
            }

            // Render cursor
            if self.focused
                && line_idx == self.doc.cursor.line
                && line_idx < self.doc.buffer.line_count()
            {
                let cursor_x =
                    area.x + self.gutter_width + (self.doc.cursor.col - self.doc.scroll_x) as u16;
                if cursor_x < content_right && cursor_x >= area.x + self.gutter_width {
                    let cursor_style = Style::default()
                        .bg(self.theme.editor.cursor)
                        .fg(self.theme.editor.background)
                        .add_modifier(Modifier::BOLD);
                    let ch = if self.doc.cursor.col < self.doc.buffer.line_len(line_idx) {
                        let line = self.doc.buffer.line_text(line_idx);
                        line.chars()
                            .nth(self.doc.cursor.col)
                            .unwrap_or(' ')
                            .to_string()
                    } else {
                        " ".to_string()
                    };
                    buf.set_string(cursor_x, y, ch, cursor_style);
                }
            }

            // Render ghost text (inline suggestion) after cursor on current line
            if line_idx == self.doc.cursor.line {
                if let Some(ghost) = self.ghost_text {
                    let content_start_x = area.x + self.gutter_width;
                    let cursor_screen_col = (self.doc.cursor.col as i32 - self.doc.scroll_x as i32).max(0) as u16;
                    let ghost_start_x = content_start_x + cursor_screen_col + 1; // one cell after cursor
                    let start_skip = if self.doc.cursor.col <= self.doc.scroll_x {
                        self.doc.scroll_x - self.doc.cursor.col
                    } else {
                        0
                    };
                    let ghost_style = Style::default()
                        .fg(self.theme.ui.ai_ghost_text)
                        .bg(self.theme.editor.current_line_bg)
                        .add_modifier(Modifier::DIM);
                    let max_x = content_right;
                    let mut x = ghost_start_x;
                    for ch in ghost.chars().skip(start_skip) {
                        if x >= max_x {
                            break;
                        }
                        buf.set_string(x, y, ch.to_string(), ghost_style);
                        x += 1;
                    }
                }
            }
        }

        // Render completion dropdown below cursor line
        if let Some((items, selected)) = self.completion_dropdown {
            if items.is_empty() {
                return;
            }
            let cursor_row = (self.doc.cursor.line as i32 - self.doc.scroll_y as i32).max(0) as u16;
            let drop_y = area.y + cursor_row + 1;
            if drop_y >= area.y + area.height {
                return;
            }
            let max_h = (area.y + area.height - drop_y).min(10).min(items.len() as u16);
            let content_x = area.x + self.gutter_width;
            let strip_cols = u16::from(
                self.show_match_strip
                    && !self.search.match_lines.is_empty()
                    && area.width > self.gutter_width + 2,
            );
            let drop_width = area
                .width
                .saturating_sub(self.gutter_width)
                .saturating_sub(strip_cols)
                .min(40);
            let bg = self.theme.editor.background;
            let selected_bg = self.theme.ui.search_current_bg;
            let fg = self.theme.editor.foreground;
            for (i, item) in items.iter().take(max_h as usize).enumerate() {
                let y = drop_y + i as u16;
                let row_style = if i == selected {
                    Style::default().fg(fg).bg(selected_bg)
                } else {
                    Style::default().fg(fg).bg(bg)
                };
                let truncated: String = item.chars().take(drop_width as usize - 2).collect();
                let line_content = format!(" {} ", truncated);
                for (j, ch) in line_content.chars().enumerate() {
                    let x = content_x + j as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, ch.to_string(), row_style);
                    }
                }
                for x in (content_x + line_content.len() as u16)..(content_x + drop_width) {
                    if x < area.x + area.width {
                        buf.set_string(x, y, " ", Style::default().bg(bg));
                    }
                }
            }
        }
    }
}
