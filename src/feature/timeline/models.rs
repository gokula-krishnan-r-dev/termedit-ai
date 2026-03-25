use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The edit operation, mapping to our internal EditCommand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimelineOp {
    Insert { pos: usize, text: String },
    Delete { pos: usize, text: String },
    Replace { pos: usize, old_text: String, new_text: String },
}

/// A specific point of change in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEvent {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub op: TimelineOp,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

/// A full copy of the file at a specific timeline ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub last_id: u64,
    pub content: String,
}

/// Event sent from the editor to the background timeline worker.
#[derive(Debug, Clone)]
pub enum TimelineEvent {
    Init {
        content: String,
    },
    Edit {
        op: TimelineOp,
        cursor_line: usize,
        cursor_col: usize,
    },
    ForceSnapshot,
    Shutdown,
}
