import re

with open("src/core/document.rs", "r") as f:
    text = f.read()

# Add imports for timeline
text = text.replace(
    "use crate::feature::search::SearchMatch;",
    "use crate::feature::search::SearchMatch;\n#[cfg(feature = \"timeline\")]\nuse crate::feature::timeline::TimelineSender;\n#[cfg(feature = \"timeline\")]\nuse crate::feature::timeline::models::{TimelineOp, TimelineEvent};"
)

# Add timeline_sender to Document
text = text.replace(
    "    pub scroll_x: usize,\n}",
    "    pub scroll_x: usize,\n    #[cfg(feature = \"timeline\")]\n    pub timeline_sender: Option<TimelineSender>,\n}"
)

text = text.replace(
    "            scroll_x: 0,\n        }",
    "            scroll_x: 0,\n            #[cfg(feature = \"timeline\")]\n            timeline_sender: None,\n        }",
    1 # only first occurrence (in new)
)

text = text.replace(
    "        let language = crate::feature::language::detect_language(path, &buffer);",
    """        let language = crate::feature::language::detect_language(path, &buffer);
        #[cfg(feature = "timeline")]
        let timeline_sender = if path.starts_with(dirs::data_local_dir().unwrap_or_default()) {
            None
        } else {
            let sender = crate::feature::timeline::start_worker(path.to_path_buf());
            sender.send_init(buffer.to_string());
            Some(sender)
        };"""
)

# Open method return body
text = text.replace(
    "            scroll_x: 0,\n        })\n    }",
    "            scroll_x: 0,\n            #[cfg(feature = \"timeline\")]\n            timeline_sender,\n        })\n    }"
)

# Add helper methods
helper = """
    #[cfg(feature = "timeline")]
    pub fn notify_timeline(&self, op: TimelineOp) {
        if let Some(sender) = &self.timeline_sender {
            sender.send_raw_event(TimelineEvent::Edit {
                op,
                cursor_line: self.cursor.line,
                cursor_col: self.cursor.col,
            });
        }
    }

    pub fn refresh_language(&mut self) {"""

text = text.replace("    pub fn refresh_language(&mut self) {", helper)

# Replace history.record
pattern = re.compile(r"self\.history\.record\(\s*(EditCommand::[a-zA-Z]+\s*\{[^}]+\}),\s*self\.cursor\.line,\s*self\.cursor\.col,?\s*\);")

def replacer(match):
    cmd_str = match.group(1)
    return """#[cfg(feature = "timeline")]
        {
            let op = match &(""" + cmd_str + """) {
                EditCommand::Insert { pos, text } => TimelineOp::Insert { pos: *pos, text: text.clone() },
                EditCommand::Delete { pos, text } => TimelineOp::Delete { pos: *pos, text: text.clone() },
                EditCommand::Replace { pos, old_text, new_text } => TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() },
            };
            self.notify_timeline(op);
        }
        self.history.record(
            """ + cmd_str + """,
            self.cursor.line,
            self.cursor.col,
        );"""

text = pattern.sub(replacer, text)


# Undo patch
undo_patch = """
                EditCommand::Insert { pos, text } => {
                    self.buffer
                        .delete(pos, pos + text.chars().count());
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Delete { pos, text });
                }
                EditCommand::Delete { pos, text } => {
                    self.buffer.insert(pos, &text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Insert { pos, text });
                }
                EditCommand::Replace {
                    pos,
                    old_text,
                    new_text,
                } => {
                    self.buffer
                        .delete(pos, pos + new_text.chars().count());
                    self.buffer.insert(pos, &old_text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Replace { pos, old_text: new_text, new_text: old_text });
                }
"""

text = re.sub(
    r"EditCommand::Insert \{ pos, text \} => \{\s*self\.buffer\s*\.delete\(pos, pos \+ text\.chars\(\)\.count\(\)\);\s*\}\s*EditCommand::Delete \{ pos, text \} => \{\s*self\.buffer\.insert\(pos, &text\);\s*\}\s*EditCommand::Replace \{\s*pos,\s*old_text,\s*new_text,\s*\} => \{\s*self\.buffer\s*\.delete\(pos, pos \+ new_text\.chars\(\)\.count\(\)\);\s*self\.buffer\.insert\(pos, &old_text\);\s*\}",
    undo_patch,
    text
)

redo_patch = """
                EditCommand::Insert { pos, ref text } => {
                    self.buffer.insert(*pos, text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Insert { pos: *pos, text: text.clone() });
                    let end = pos + text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }
                EditCommand::Delete { pos, ref text } => {
                    self.buffer
                        .delete(*pos, pos + text.chars().count());
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Delete { pos: *pos, text: text.clone() });
                    self.cursor.line = self.buffer.char_to_line(*pos);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = pos - line_start;
                }
                EditCommand::Replace {
                    pos,
                    ref old_text,
                    ref new_text,
                } => {
                    self.buffer
                        .delete(*pos, pos + old_text.chars().count());
                    self.buffer.insert(*pos, new_text);
                    #[cfg(feature = "timeline")]
                    self.notify_timeline(TimelineOp::Replace { pos: *pos, old_text: old_text.clone(), new_text: new_text.clone() });
                    let end = pos + new_text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }"""

redo_orig = """                EditCommand::Insert { pos, ref text } => {
                    self.buffer.insert(*pos, text);
                    let end = *pos + text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }
                EditCommand::Delete { pos, ref text } => {
                    self.buffer
                        .delete(*pos, *pos + text.chars().count());
                    self.cursor.line = self.buffer.char_to_line(*pos);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = *pos - line_start;
                }
                EditCommand::Replace {
                    pos,
                    ref old_text,
                    ref new_text,
                } => {
                    self.buffer
                        .delete(*pos, *pos + old_text.chars().count());
                    self.buffer.insert(*pos, new_text);
                    let end = *pos + new_text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }"""

text = text.replace(redo_orig, redo_patch)
# Try also the version with pos instead of *pos since I reverted
redo_orig_2 = """                EditCommand::Insert { pos, ref text } => {
                    self.buffer.insert(pos, text);
                    let end = pos + text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }
                EditCommand::Delete { pos, ref text } => {
                    self.buffer
                        .delete(pos, pos + text.chars().count());
                    self.cursor.line = self.buffer.char_to_line(pos);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = pos - line_start;
                }
                EditCommand::Replace {
                    pos,
                    ref old_text,
                    ref new_text,
                } => {
                    self.buffer
                        .delete(pos, pos + old_text.chars().count());
                    self.buffer.insert(pos, new_text);
                    let end = pos + new_text.chars().count();
                    self.cursor.line = self.buffer.char_to_line(end);
                    let line_start = self.buffer.line_to_char(self.cursor.line);
                    self.cursor.col = end - line_start;
                }"""
text = text.replace(redo_orig_2, redo_patch.replace("*pos", "pos"))


with open("src/core/document.rs", "w") as f:
    f.write(text)

with open("src/feature/mod.rs", "r") as f:
    mod_text = f.read()
if "pub mod timeline" not in mod_text:
    mod_text += "\\n#[cfg(feature = \\\"timeline\\\")]\\npub mod timeline;\\n"
    with open("src/feature/mod.rs", "w") as f:
        f.write(mod_text)
