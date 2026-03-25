//! Run `git status --porcelain` and collect changed file paths (for `--open-git-changed`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Collect paths from `git status --porcelain` in `repo_cwd`. Skips deletions and non-files.
pub fn changed_file_paths(repo_cwd: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_cwd)
        .output()
        .map_err(|e| format!("git: {}", e))?;

    if !output.status.success() {
        let msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if msg.is_empty() {
            return Err("git status failed (is this a git repository?)".to_string());
        }
        return Err(format!("git: {}", msg));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<PathBuf> = Vec::new();

    for line in text.lines() {
        let Some(rel) = parse_porcelain_line_path(line) else {
            continue;
        };
        if !seen.insert(rel.clone()) {
            continue;
        }
        let abs = repo_cwd.join(&rel);
        if abs.is_file() {
            out.push(abs);
        }
    }

    Ok(out)
}

/// Return the worktree-relative path to open, or None to skip the line.
fn parse_porcelain_line_path(line: &str) -> Option<String> {
    let line = line.trim_end();
    if line.is_empty() {
        return None;
    }

    // Untracked: ?? path
    if line.starts_with("?? ") {
        return Some(line[3..].trim().to_string());
    }

    // Need XY + space (columns 0–2)
    let bytes = line.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    if bytes[2] != b' ' {
        return None;
    }
    let x = bytes[0] as char;
    let y = bytes[1] as char;

    // Skip deletion in work tree (path usually gone) and typical pure deletes.
    if y == 'D' {
        return None;
    }
    if x == 'D' && y == ' ' {
        return None;
    }

    let rest = line[3..].trim_start();
    if rest.is_empty() {
        return None;
    }

    // Rename / copy: ... -> newname  or  ... => newname
    for sep in [" -> ", " => "] {
        if let Some(idx) = rest.rfind(sep) {
            let new_side = rest[idx + sep.len()..].trim();
            if !new_side.is_empty() {
                return Some(new_side.to_string());
            }
        }
    }

    Some(rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_modified_untracked() {
        assert_eq!(
            parse_porcelain_line_path(" M src/main.rs").as_deref(),
            Some("src/main.rs")
        );
        assert_eq!(
            parse_porcelain_line_path("M  README.md").as_deref(),
            Some("README.md")
        );
        assert_eq!(
            parse_porcelain_line_path("?? foo/bar.txt").as_deref(),
            Some("foo/bar.txt")
        );
    }

    #[test]
    fn parse_skips_deleted() {
        assert_eq!(parse_porcelain_line_path(" D gone.rs"), None);
        assert_eq!(parse_porcelain_line_path("D  gone2.rs"), None);
    }

    #[test]
    fn parse_rename() {
        assert_eq!(
            parse_porcelain_line_path("R  old.txt -> new.txt").as_deref(),
            Some("new.txt")
        );
        assert_eq!(
            parse_porcelain_line_path("R  old.txt => new.txt").as_deref(),
            Some("new.txt")
        );
    }

    #[test]
    fn parse_skips_empty() {
        assert_eq!(parse_porcelain_line_path(""), None);
        assert_eq!(parse_porcelain_line_path("bad"), None);
    }
}
