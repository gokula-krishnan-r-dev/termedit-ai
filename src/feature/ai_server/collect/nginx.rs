use anyhow::Result;
use std::path::Path;

use crate::feature::ai_server::context::{CollectConfig, ConfigSnippet, ServerContext};
use walkdir::WalkDir;

pub async fn contribute(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    for root in &config.nginx_paths {
        if root.is_file() {
            add_file(ctx, config, "nginx", root).await;
        } else if root.is_dir() {
            for entry in WalkDir::new(root).max_depth(2).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let p = entry.path();
                    if looks_like_nginx_conf(p) {
                        add_file(ctx, config, "nginx", p).await;
                    }
                }
            }
        }
    }
    Ok(())
}

fn looks_like_nginx_conf(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e, "conf" | "types" | "inc"))
        .unwrap_or(false)
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
        Err(e) => ctx.notes.push(format!("nginx {}: {}", path.display(), e)),
    }
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
