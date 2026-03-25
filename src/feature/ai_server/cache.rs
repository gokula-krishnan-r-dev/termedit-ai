//! Disk cache for `ServerContext` with TTL.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::context::{CollectConfig, ServerContext};

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    pub saved_at_unix_ms: u128,
    pub context: ServerContext,
}

pub struct ContextCache {
    dir: PathBuf,
    ttl: Duration,
}

impl ContextCache {
    pub fn new(dir: PathBuf, ttl: Duration) -> Self {
        Self { dir, ttl }
    }

    pub fn cache_key(hostname: &str, config: &CollectConfig) -> String {
        let mut json =
            serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
        json.push('|');
        json.push_str(hostname);
        let h = Sha256::digest(json.as_bytes());
        format!("{:x}", h)
    }

    pub fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    pub async fn get_valid(&self, key: &str) -> Result<Option<ServerContext>> {
        let p = self.path_for(key);
        if !p.exists() {
            return Ok(None);
        }
        let meta = tokio::fs::metadata(&p).await?;
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::MAX);
        if age > self.ttl {
            let _ = tokio::fs::remove_file(&p).await;
            return Ok(None);
        }
        let raw = tokio::fs::read_to_string(&p).await?;
        let entry: CacheEntry = serde_json::from_str(&raw)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u128)
            .unwrap_or(0);
        if now.saturating_sub(entry.saved_at_unix_ms) > self.ttl.as_millis() as u128 {
            let _ = tokio::fs::remove_file(&p).await;
            return Ok(None);
        }
        Ok(Some(entry.context))
    }

    pub async fn put(&self, key: &str, ctx: &ServerContext) -> Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let entry = CacheEntry {
            saved_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u128)
                .unwrap_or(0),
            context: ctx.clone(),
        };
        let tmp = self.path_for(&format!("{key}.tmp"));
        let final_path = self.path_for(key);
        let data = serde_json::to_vec_pretty(&entry)?;
        tokio::fs::write(&tmp, &data).await?;
        tokio::fs::rename(&tmp, &final_path).await?;
        Ok(())
    }
}

pub fn default_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("termedit").join("ai-server"))
}
