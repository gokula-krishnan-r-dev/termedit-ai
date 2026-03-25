use anyhow::Result;
use std::path::Path;

use crate::feature::ai_server::context::{CollectConfig, ConfigSnippet, ServerContext};
use walkdir::WalkDir;

const MAX_UNITS: usize = 40;

pub async fn contribute(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    let mut n = 0usize;
    for root in &config.systemd_paths {
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root).max_depth(1).into_iter().filter_map(|e| e.ok()) {
            if n >= MAX_UNITS {
                ctx.notes.push(format!(
                    "systemd: stopped after {} unit files (cap)",
                    MAX_UNITS
                ));
                return Ok(());
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("service") {
                continue;
            }
            match read_capped(p, config.max_file_bytes.min(64 * 1024)).await {
                Ok((content, truncated)) => {
                    ctx.configs.push(ConfigSnippet {
                        kind: "systemd".to_string(),
                        path: p.to_string_lossy().into_owned(),
                        truncated,
                        content,
                    });
                    n += 1;
                }
                Err(e) => ctx
                    .notes
                    .push(format!("systemd {}: {}", p.display(), e)),
            }
        }
    }
    Ok(())
}

async fn read_capped(path: &Path, max: usize) -> Result<(String, bool)> {
    let meta = tokio::fs::metadata(path).await?;
    let len = meta.len() as usize;
    let truncated = len > max;
    let cap = max.min(len);
    let mut buf = vec![0u8; cap];
    let mut f = tokio::fs::File::open(path).await?;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    if truncated {
        f.seek(std::io::SeekFrom::Start((len - cap) as u64)).await?;
    }
    let n = f.read(&mut buf).await?;
    buf.truncate(n);
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}
