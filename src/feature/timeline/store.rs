use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::models::{PatchEvent, Snapshot, TimelineOp};

pub struct TimelineStore {
    dir: PathBuf,
}

impl TimelineStore {
    pub fn new(file_path: &Path) -> Self {
        let dir = super::worker::get_timeline_dir(file_path);
        Self { dir }
    }

    pub fn load_snapshots(&self) -> Vec<Snapshot> {
        let mut snaps = Vec::new();
        let path = self.dir.join("snapshots.jsonl");
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(s) = serde_json::from_str::<Snapshot>(&line) {
                    snaps.push(s);
                }
            }
        }
        snaps
    }

    pub fn load_patches(&self) -> Vec<PatchEvent> {
        let mut patches = Vec::new();
        let path = self.dir.join("patches.jsonl");
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(p) = serde_json::from_str::<PatchEvent>(&line) {
                    patches.push(p);
                }
            }
        }
        patches
    }

    pub fn reconstruct_state(
        &self,
        target_id: u64,
        snapshots: &[Snapshot],
        patches: &[PatchEvent],
    ) -> String {
        // Find nearest snapshot before or equal to target_id
        let mut best_snap = None;
        for snap in snapshots.iter().rev() {
            if snap.last_id <= target_id {
                best_snap = Some(snap);
                break;
            }
        }

        let mut current_id = 0;
        let mut rope = if let Some(snap) = best_snap {
            current_id = snap.last_id + 1;
            ropey::Rope::from_str(&snap.content)
        } else {
            ropey::Rope::new()
        };

        // Replay patches
        for patch in patches {
            if patch.id >= current_id && patch.id <= target_id {
                match &patch.op {
                    TimelineOp::Insert { pos, text } => {
                        if *pos <= rope.len_chars() {
                            rope.insert(*pos, text);
                        }
                    }
                    TimelineOp::Delete { pos, text } => {
                        let end = *pos + text.chars().count();
                        if end <= rope.len_chars() {
                            rope.remove(*pos..end);
                        }
                    }
                    TimelineOp::Replace {
                        pos,
                        old_text,
                        new_text,
                    } => {
                        let end = *pos + old_text.chars().count();
                        if end <= rope.len_chars() {
                            rope.remove(*pos..end);
                            rope.insert(*pos, new_text);
                        }
                    }
                }
            }
        }
        rope.to_string()
    }
}
