use std::collections::hash_map::DefaultHasher;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tokio::sync::mpsc;

use crate::core::history::EditCommand;

use super::models::{PatchEvent, Snapshot, TimelineEvent, TimelineOp};

pub struct TimelineSender {
    tx: mpsc::UnboundedSender<TimelineEvent>,
}

impl TimelineSender {
    pub fn send_edit(&self, cmd: &EditCommand, cursor_line: usize, cursor_col: usize) {
        let op = match cmd {
            EditCommand::Insert { pos, text } => TimelineOp::Insert {
                pos: *pos,
                text: text.clone(),
            },
            EditCommand::Delete { pos, text } => TimelineOp::Delete {
                pos: *pos,
                text: text.clone(),
            },
            EditCommand::Replace {
                pos,
                old_text,
                new_text,
            } => TimelineOp::Replace {
                pos: *pos,
                old_text: old_text.clone(),
                new_text: new_text.clone(),
            },
        };
        let _ = self.tx.send(TimelineEvent::Edit {
            op,
            cursor_line,
            cursor_col,
        });
    }

    pub fn force_snapshot(&self) {
        let _ = self.tx.send(TimelineEvent::ForceSnapshot);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(TimelineEvent::Shutdown);
    }

    pub fn send_init(&self, content: String) {
        let _ = self.tx.send(TimelineEvent::Init { content });
    }

    pub fn send_raw_event(&self, evt: TimelineEvent) {
        let _ = self.tx.send(evt);
    }
}

pub(crate) fn get_timeline_dir(file_path: &Path) -> PathBuf {
    let abs = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    abs.to_string_lossy().hash(&mut hasher);
    let hash_val = hasher.finish();

    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("termedit");
    dir.push("timeline");
    dir.push(format!("{:x}", hash_val));
    dir
}

pub fn start_worker(file_path: PathBuf) -> TimelineSender {
    let (tx, mut rx) = mpsc::unbounded_channel::<TimelineEvent>();
    let dir = get_timeline_dir(&file_path);
    fs::create_dir_all(&dir).unwrap_or_default();

    let patches_file = dir.join("patches.jsonl");
    let snapshots_file = dir.join("snapshots.jsonl");

    tokio::spawn(async move {
        let mut patch_id: u64 = 0;
        let mut shadow_rope = ropey::Rope::new();
        let mut changes_since_snapshot = 0;
        const SNAPSHOT_INTERVAL: usize = 50;

        // Note: in a real implementation we should recover `patch_id` from existing files.
        // For simplicity, we can load the last ID from the patches file on startup.
        if let Ok(content) = std::fs::read_to_string(&patches_file) {
            for line in content.lines() {
                if let Ok(p) = serde_json::from_str::<PatchEvent>(line) {
                    if p.id >= patch_id {
                        patch_id = p.id + 1;
                    }
                }
            }
        }

        while let Some(evt) = rx.recv().await {
            match evt {
                TimelineEvent::Init { content } => {
                    shadow_rope = ropey::Rope::from_str(&content);
                    if patch_id == 0 {
                        // Write initial snapshot for reconstructing from beginning
                        let snapshot = Snapshot {
                            last_id: 0,
                            content: shadow_rope.to_string(),
                        };
                        if let Ok(mut file) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&snapshots_file)
                        {
                            if let Ok(json) = serde_json::to_string(&snapshot) {
                                let _ = writeln!(file, "{}", json);
                            }
                        }
                    }
                }
                TimelineEvent::Edit {
                    op,
                    cursor_line,
                    cursor_col,
                } => {
                    let ts = Utc::now();
                    let patch = PatchEvent {
                        id: patch_id,
                        timestamp: ts,
                        op: op.clone(),
                        cursor_line,
                        cursor_col,
                    };

                    // Apply to shadow rope
                    match &op {
                        TimelineOp::Insert { pos, text } => {
                            shadow_rope.insert(*pos, text);
                        }
                        TimelineOp::Delete { pos, text } => {
                            let end = *pos + text.chars().count();
                            shadow_rope.remove(*pos..end);
                        }
                        TimelineOp::Replace {
                            pos,
                            old_text,
                            new_text,
                        } => {
                            let end = *pos + old_text.chars().count();
                            shadow_rope.remove(*pos..end);
                            shadow_rope.insert(*pos, new_text);
                        }
                    }

                    // Write patch
                    if let Ok(mut file) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&patches_file)
                    {
                        if let Ok(json) = serde_json::to_string(&patch) {
                            let _ = writeln!(file, "{}", json);
                        }
                    }

                    patch_id += 1;
                    changes_since_snapshot += 1;

                    if changes_since_snapshot >= SNAPSHOT_INTERVAL {
                        changes_since_snapshot = 0;
                        let snapshot = Snapshot {
                            last_id: patch_id - 1,
                            content: shadow_rope.to_string(),
                        };
                        if let Ok(mut file) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&snapshots_file)
                        {
                            if let Ok(json) = serde_json::to_string(&snapshot) {
                                let _ = writeln!(file, "{}", json);
                            }
                        }
                    }
                }
                TimelineEvent::ForceSnapshot => {
                    changes_since_snapshot = 0;
                    if patch_id > 0 {
                        let snapshot = Snapshot {
                            last_id: patch_id - 1,
                            content: shadow_rope.to_string(),
                        };
                        if let Ok(mut file) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&snapshots_file)
                        {
                            if let Ok(json) = serde_json::to_string(&snapshot) {
                                let _ = writeln!(file, "{}", json);
                            }
                        }
                    }
                }
                TimelineEvent::Shutdown => {
                    break;
                }
            }
        }
    });

    TimelineSender { tx }
}
