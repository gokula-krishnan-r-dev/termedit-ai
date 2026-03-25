use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::Semaphore;
use walkdir::WalkDir;

use crate::feature::ai_server::context::{CollectConfig, LogSummary, ServerContext};
use crate::feature::ai_server::summarize::summarize_log_bytes;

pub async fn contribute(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    let sem = std::sync::Arc::new(Semaphore::new(config.concurrency.max(1)));
    let mut paths: Vec<PathBuf> = vec![];

    for root in &config.log_roots {
        if !root.exists() {
            ctx.notes.push(format!("log root missing or unreadable: {}", root.display()));
            continue;
        }
        let mut count = 0usize;
        for entry in WalkDir::new(root).max_depth(4).into_iter().filter_map(|e| e.ok()) {
            if count >= config.max_log_files {
                break;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path().to_path_buf();
            if is_plain_textish(&p) {
                paths.push(p);
                count += 1;
            }
        }
    }

    paths.sort();
    paths.dedup();

    let mut total_bytes = 0usize;
    let mut summaries = vec![];

    for p in paths {
        let permit = sem.clone().acquire_owned().await?;
        let cfg = config.clone();
        let path_clone = p.clone();
        let j = tokio::spawn(async move {
            let _permit = permit;
            read_one_log(&path_clone, &cfg).await
        });
        match j.await {
            Ok(Ok(Some(s))) => {
                total_bytes += s.text.len();
                summaries.push(s);
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => ctx.notes.push(format!("log {}: {}", p.display(), e)),
            Err(e) => ctx.notes.push(format!("log task {}: {}", p.display(), e)),
        }
        if total_bytes >= config.max_log_total_bytes {
            ctx.notes.push(format!(
                "log collection stopped at total ~{} bytes (cap {})",
                total_bytes, config.max_log_total_bytes
            ));
            break;
        }
    }

    summaries.sort_by(|a, b| a.path.cmp(&b.path));
    ctx.logs = summaries;
    Ok(())
}

fn is_plain_textish(p: &Path) -> bool {
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.contains(".log")
        || name.ends_with(".txt")
        || name.ends_with(".err")
        || name.ends_with(".out")
    {
        return true;
    }
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("log" | "txt" | "err" | "out")
    )
}

async fn read_one_log(path: &Path, config: &CollectConfig) -> Result<Option<LogSummary>> {
    let meta = match fs::metadata(path).await {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let len = meta.len() as usize;
    let cap = config.max_file_bytes.min(len);
    let mut file = fs::File::open(path).await?;
    let skip = len.saturating_sub(cap);
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    if skip > 0 {
        file.seek(std::io::SeekFrom::Start(skip as u64)).await?;
    }
    let mut buf = vec![0u8; cap.min(config.max_file_bytes)];
    let n = file.read(&mut buf).await?;
    buf.truncate(n);
    let text = String::from_utf8_lossy(&buf).into_owned();
    let truncated = len > config.max_file_bytes;
    let (sample, lines) = summarize_log_bytes(&text, config.max_lines_per_log);
    Ok(Some(LogSummary {
        path: path.to_string_lossy().into_owned(),
        truncated,
        line_count_in_sample: lines,
        text: sample,
    }))
}
