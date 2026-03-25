/// Session persistence: save and restore open files, active tab, cursor, and scroll.
///
/// Unique feature: restart the editor and return to the exact same state—no other
/// nano-style TUI editor does this out of the box. Solves real developer pain:
/// reopening many files and losing cursor/scroll after a crash or exit.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::core::document::Document;

/// Per-document state to restore (cursor and scroll).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentState {
    pub line: usize,
    pub col: usize,
    pub scroll_y: usize,
    pub scroll_x: usize,
}

/// Full session snapshot: file paths (absolute when possible) and state per document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    /// Open file paths (only file-backed documents; untitled are skipped).
    pub paths: Vec<String>,
    /// Index of the active tab (into paths).
    pub active_tab: usize,
    /// Cursor and scroll per document; length must match paths.
    pub states: Vec<DocumentState>,
}

impl SessionState {
    /// Save session to a JSON file. Silently no-op on any error (e.g. no config dir).
    pub fn save_to(&self, path: &Path) -> bool {
        if self.paths.is_empty() {
            return true;
        }
        let json = match serde_json::to_string_pretty(self) {
            Ok(j) => j,
            Err(_) => return false,
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, json).is_ok()
    }

    /// Load session from a JSON file. Returns None if file missing or invalid.
    pub fn load_from(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let state: SessionState = serde_json::from_str(&content).ok()?;
        if state.states.len() != state.paths.len() {
            return None;
        }
        Some(state)
    }
}

/// Build session state from current documents and active tab.
/// Only includes documents with a file path; uses canonical paths when possible.
/// Active tab is mapped to the index among saved (file-backed) docs.
pub fn snapshot(documents: &[Document], active_tab: usize) -> SessionState {
    let mut paths = Vec::new();
    let mut states = Vec::new();
    let mut saved_active = 0usize;
    for (i, doc) in documents.iter().enumerate() {
        let path = match &doc.buffer.file_path {
            Some(p) => p.clone(),
            None => continue,
        };
        let abs = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(s) = abs.to_str() {
            if i == active_tab {
                saved_active = paths.len();
            }
            paths.push(s.to_string());
            states.push(DocumentState {
                line: doc.cursor.line,
                col: doc.cursor.col,
                scroll_y: doc.scroll_y,
                scroll_x: doc.scroll_x,
            });
        }
    }
    let n = paths.len();
    SessionState {
        paths,
        active_tab: saved_active.min(n.saturating_sub(1)),
        states,
    }
}

/// Default session file path under config dir.
pub fn default_session_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("termedit").join("session.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::document::Document;

    #[test]
    fn test_snapshot_empty() {
        let docs = vec![Document::new()];
        let state = snapshot(&docs, 0);
        assert!(state.paths.is_empty());
        assert!(state.states.is_empty());
    }

    #[test]
    fn test_session_state_roundtrip() {
        let state = SessionState {
            paths: vec!["/a/b.rs".to_string(), "/c/d.py".to_string()],
            active_tab: 1,
            states: vec![
                DocumentState {
                    line: 0,
                    col: 0,
                    scroll_y: 0,
                    scroll_x: 0,
                },
                DocumentState {
                    line: 10,
                    col: 4,
                    scroll_y: 5,
                    scroll_x: 2,
                },
            ],
        };
        let json = serde_json::to_string(&state).unwrap();
        let loaded: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.paths, state.paths);
        assert_eq!(loaded.active_tab, state.active_tab);
        assert_eq!(loaded.states.len(), state.states.len());
    }
}
