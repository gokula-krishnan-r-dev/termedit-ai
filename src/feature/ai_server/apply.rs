//! Safe apply: preview, confirm, backup, write.

use anyhow::{Context, Result};
use similar::TextDiff;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::response::{AssistantPlan, FileEdit};
use super::ui;

pub fn offer_apply_plan(
    plan: &AssistantPlan,
    dry_run: bool,
) -> Result<()> {
    if plan.file_edits.is_empty() {
        return Ok(());
    }
    ui::print_section("Proposed file changes");
    for edit in &plan.file_edits {
        preview_one(edit)?;
    }
    if dry_run {
        println!("\n(dry-run: no files were modified)");
        return Ok(());
    }
    let proceed = dialoguer::Confirm::new()
        .with_prompt("Apply file changes (backups will be created)?")
        .default(false)
        .interact()
        .unwrap_or(false);
    if !proceed {
        println!("Skipped applying files.");
        return Ok(());
    }
    for edit in &plan.file_edits {
        apply_one(edit)?;
    }
    println!("Applied {} file edit(s).", plan.file_edits.len());
    Ok(())
}

fn preview_one(edit: &FileEdit) -> Result<()> {
    let path = Path::new(&edit.path);
    let original = if path.exists() {
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    let new_text = resolved_new_content(&original, edit)?;
    let td = TextDiff::from_lines(original.as_str(), new_text.as_str());
    let mut ud = td.unified_diff();
    let u = ud.context_radius(3).header(&edit.path, &edit.path);
    let diff_str = u.to_string();
    ui::print_diff(&diff_str);
    Ok(())
}

fn resolved_new_content(original: &str, edit: &FileEdit) -> Result<String> {
    if let Some(full) = &edit.new_content {
        return Ok(full.clone());
    }
    if let Some(patch_str) = &edit.unified_diff {
        let patch = diffy::Patch::from_str(patch_str.as_str())
            .map_err(|e| anyhow::anyhow!("invalid unified diff: {}", e))?;
        return diffy::apply(original, &patch).map_err(|e| anyhow::anyhow!("diff apply: {}", e));
    }
    anyhow::bail!("file_edits entry needs unified_diff or new_content: {}", edit.path);
}

fn backup_path(target: &Path) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut s = target.to_string_lossy().into_owned();
    s.push_str(&format!(".bak.{ts}"));
    PathBuf::from(s)
}

fn apply_one(edit: &FileEdit) -> Result<()> {
    let path = Path::new(&edit.path);
    let original = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?
    } else {
        String::new()
    };
    let new_text = resolved_new_content(&original, edit)?;
    if path.exists() {
        let bak = backup_path(path);
        fs::copy(path, &bak)
            .with_context(|| format!("backup {} -> {}", path.display(), bak.display()))?;
        println!("Backup: {}", bak.display());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let tmp = path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".{fname}.termedit-{ts}.tmp"));
    fs::write(&tmp, &new_text).with_context(|| format!("write tmp {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    println!("Wrote {}", path.display());
    let _ = std::io::stdout().flush();
    Ok(())
}
