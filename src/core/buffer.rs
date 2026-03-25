/// Rope-based text buffer using the `ropey` crate.
///
/// Provides O(log n) insert/delete operations, encoding detection,
/// and line-ending preservation for production-quality text editing.

use ropey::Rope;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::error::{Result, TermEditError};

/// Detected file encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Latin1,
}

impl std::fmt::Display for Encoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Encoding::Utf8 => write!(f, "UTF-8"),
            Encoding::Utf8Bom => write!(f, "UTF-8 BOM"),
            Encoding::Latin1 => write!(f, "Latin1"),
        }
    }
}

/// Detected line ending style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix-style `\n`
    Lf,
    /// Windows-style `\r\n`
    Crlf,
}

impl std::fmt::Display for LineEnding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineEnding::Lf => write!(f, "LF"),
            LineEnding::Crlf => write!(f, "CRLF"),
        }
    }
}

/// Core text buffer backed by a `ropey::Rope`.
///
/// All text operations are O(log n) via the rope data structure.
/// The buffer tracks file path, modification state, encoding, and line endings.
#[derive(Debug)]
pub struct Buffer {
    /// The rope data structure holding the text content.
    pub rope: Rope,
    /// Path to the file on disk, if any.
    pub file_path: Option<PathBuf>,
    /// Whether the buffer has been modified since last save.
    pub modified: bool,
    /// Detected file encoding.
    pub encoding: Encoding,
    /// Detected line ending style.
    pub line_ending: LineEnding,
}

impl Buffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            file_path: None,
            modified: false,
            encoding: Encoding::Utf8,
            line_ending: if cfg!(windows) {
                LineEnding::Crlf
            } else {
                LineEnding::Lf
            },
        }
    }

    /// Load a buffer from a file path.
    ///
    /// Detects encoding and line endings automatically.
    pub fn from_file(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).map_err(TermEditError::Io)?;

        // Detect encoding
        let (text, encoding) = Self::decode_bytes(&bytes)?;

        // Detect line endings
        let line_ending = Self::detect_line_ending(&text);

        // Normalize to LF for internal storage (ropey uses LF)
        let normalized = text.replace("\r\n", "\n");

        let rope = Rope::from_str(&normalized);

        Ok(Self {
            rope,
            file_path: Some(path.to_path_buf()),
            modified: false,
            encoding,
            line_ending,
        })
    }

    /// Decode raw bytes into a String, detecting encoding.
    fn decode_bytes(bytes: &[u8]) -> Result<(String, Encoding)> {
        // Check for UTF-8 BOM
        if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
            let text = String::from_utf8(bytes[3..].to_vec())
                .map_err(|e| TermEditError::UnsupportedEncoding(e.to_string()))?;
            return Ok((text, Encoding::Utf8Bom));
        }

        // Try UTF-8 first
        match String::from_utf8(bytes.to_vec()) {
            Ok(text) => Ok((text, Encoding::Utf8)),
            Err(_) => {
                // Fallback to Latin1 (ISO-8859-1) — every byte is valid
                let text: String = bytes.iter().map(|&b| b as char).collect();
                Ok((text, Encoding::Latin1))
            }
        }
    }

    /// Detect line ending style from text content.
    fn detect_line_ending(text: &str) -> LineEnding {
        let crlf_count = text.matches("\r\n").count();
        let lf_count = text.matches('\n').count().saturating_sub(crlf_count);

        if crlf_count > lf_count {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        }
    }

    /// Insert text at the given character index.
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let idx = char_idx.min(self.rope.len_chars());
        self.rope.insert(idx, text);
        self.modified = true;
    }

    /// Delete a range of characters `[start..end)`.
    pub fn delete(&mut self, start: usize, end: usize) {
        let start = start.min(self.rope.len_chars());
        let end = end.min(self.rope.len_chars());
        if start < end {
            self.rope.remove(start..end);
            self.modified = true;
        }
    }

    /// Get the total number of lines.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Convert a line index to a character index (start of line).
    pub fn line_to_char(&self, line_idx: usize) -> usize {
        let line_idx = line_idx.min(self.rope.len_lines().saturating_sub(1));
        self.rope.line_to_char(line_idx)
    }

    /// Convert a character index to a line index.
    pub fn char_to_line(&self, char_idx: usize) -> usize {
        let char_idx = char_idx.min(self.rope.len_chars());
        self.rope.char_to_line(char_idx)
    }

    /// Get the length of a specific line (in characters, excluding newline).
    pub fn line_len(&self, line_idx: usize) -> usize {
        if line_idx >= self.rope.len_lines() {
            return 0;
        }
        let line = self.rope.line(line_idx);
        let len = line.len_chars();
        // Subtract trailing newline characters
        if len > 0 {
            let last = line.char(len - 1);
            if last == '\n' {
                if len > 1 && line.char(len - 2) == '\r' {
                    len - 2
                } else {
                    len - 1
                }
            } else {
                len
            }
        } else {
            0
        }
    }

    /// Get the text content of a specific line (without newline).
    pub fn line_text(&self, line_idx: usize) -> String {
        if line_idx >= self.rope.len_lines() {
            return String::new();
        }
        let line = self.rope.line(line_idx);
        let text: String = line.chars().collect();
        text.trim_end_matches(&['\n', '\r'][..]).to_string()
    }

    /// Get the total number of characters.
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// Get all text as a String.
    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }

    /// Save the buffer to its file path.
    pub fn save(&mut self) -> Result<()> {
        let path = self
            .file_path
            .as_ref()
            .ok_or_else(|| TermEditError::Buffer("No file path set".to_string()))?
            .clone();
        self.save_to(&path)
    }

    /// Save the buffer to a specific file path.
    pub fn save_to(&mut self, path: &Path) -> Result<()> {
        let mut text = self.rope.to_string();

        // Convert line endings back to original format
        if self.line_ending == LineEnding::Crlf {
            text = text.replace('\n', "\r\n");
        }

        // Handle encoding
        let bytes = match self.encoding {
            Encoding::Utf8 => text.into_bytes(),
            Encoding::Utf8Bom => {
                let mut bytes = vec![0xEF, 0xBB, 0xBF];
                bytes.extend_from_slice(text.as_bytes());
                bytes
            }
            Encoding::Latin1 => text.bytes().collect(),
        };

        let file = fs::File::create(path).map_err(TermEditError::Io)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&bytes).map_err(TermEditError::Io)?;
        writer.flush().map_err(TermEditError::Io)?;

        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        Ok(())
    }

    /// Get the file name for display (basename or "Untitled").
    pub fn display_name(&self) -> String {
        self.file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string()
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_new_empty_buffer() {
        let buf = Buffer::new();
        assert_eq!(buf.len_chars(), 0);
        assert!(buf.is_empty());
        assert!(!buf.modified);
        assert!(buf.file_path.is_none());
    }

    #[test]
    fn test_insert_text() {
        let mut buf = Buffer::new();
        buf.insert(0, "Hello");
        assert_eq!(buf.to_string(), "Hello");
        assert!(buf.modified);

        buf.insert(5, " World");
        assert_eq!(buf.to_string(), "Hello World");

        // Insert in the middle
        buf.insert(5, ",");
        assert_eq!(buf.to_string(), "Hello, World");
    }

    #[test]
    fn test_delete_text() {
        let mut buf = Buffer::new();
        buf.insert(0, "Hello World");
        buf.delete(5, 11);
        assert_eq!(buf.to_string(), "Hello");
    }

    #[test]
    fn test_line_operations() {
        let mut buf = Buffer::new();
        buf.insert(0, "Line 1\nLine 2\nLine 3\n");

        assert_eq!(buf.line_count(), 4); // 3 lines + trailing empty
        assert_eq!(buf.line_text(0), "Line 1");
        assert_eq!(buf.line_text(1), "Line 2");
        assert_eq!(buf.line_text(2), "Line 3");
        assert_eq!(buf.line_len(0), 6);
    }

    #[test]
    fn test_line_indexing() {
        let mut buf = Buffer::new();
        buf.insert(0, "aaa\nbbb\nccc");

        assert_eq!(buf.line_to_char(0), 0);
        assert_eq!(buf.line_to_char(1), 4); // 'a','a','a','\n'
        assert_eq!(buf.line_to_char(2), 8);

        assert_eq!(buf.char_to_line(0), 0);
        assert_eq!(buf.char_to_line(4), 1);
        assert_eq!(buf.char_to_line(8), 2);
    }

    #[test]
    fn test_insert_beyond_length() {
        let mut buf = Buffer::new();
        buf.insert(0, "Hi");
        // Insert beyond length should clamp
        buf.insert(100, " there");
        assert_eq!(buf.to_string(), "Hi there");
    }

    #[test]
    fn test_display_name_untitled() {
        let buf = Buffer::new();
        assert_eq!(buf.display_name(), "Untitled");
    }
}
