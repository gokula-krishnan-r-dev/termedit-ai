use anyhow::Result;
use std::path::Path;

use crate::feature::ai_server::context::{CollectConfig, ConfigSnippet, ServerContext};

pub async fn contribute(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    for p in &config.docker_paths {
        if !p.exists() {
            continue;
        }
        if p.is_file() {
            add_file(ctx, config, "docker", p).await;
        }
    }
    Ok(())
}

async fn add_file(ctx: &mut ServerContext, config: &CollectConfig, kind: &str, path: &Path) {
    match read_capped(path, config.max_file_bytes).await {
        Ok((content, truncated)) => {
            ctx.configs.push(ConfigSnippet {
                kind: kind.to_string(),
                path: path.to_string_lossy().into_owned(),
                truncated,
                content,
            });
        }
        Err(e) => ctx.notes.push(format!("docker {}: {}", path.display(), e)),
    }
}

async fn read_capped(path: &Path, max: usize) -> anyhow::Result<(String, bool)> {
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
