use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::feature::ai_server::context::{CollectConfig, DotenvLine, ServerContext};

pub async fn contribute(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    let mut seen = std::collections::HashSet::<PathBuf>::new();
    let mut out = vec![];

    for base in &config.env_search_paths {
        let mut cur = Some(base.as_path());
        while let Some(dir) = cur {
            for name in [".env", ".env.local", ".env.production"] {
                let p = dir.join(name);
                if p.is_file() && seen.insert(p.clone()) {
                    if let Ok(lines) = parse_env_file(&p, config.include_secrets).await {
                        out.extend(lines);
                    }
                }
            }
            cur = dir.parent();
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path).then(a.raw_redacted.cmp(&b.raw_redacted)));
    ctx.dotenv = out;
    Ok(())
}

async fn parse_env_file(path: &Path, include_secrets: bool) -> Result<Vec<DotenvLine>> {
    let text = tokio::fs::read_to_string(path).await?;
    let mut v = vec![];
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let (key, raw) = if let Some((k, rest)) = t.split_once('=') {
            let k = k.trim();
            let rest = rest.trim();
            if include_secrets {
                (
                    Some(k.to_string()),
                    format!("{}={}", k, rest),
                )
            } else {
                (Some(k.to_string()), format!("{}=REDACTED", k))
            }
        } else {
            (None, t.to_string())
        };
        v.push(DotenvLine {
            path: path.to_string_lossy().into_owned(),
            key,
            raw_redacted: raw,
        });
    }
    Ok(v)
}
