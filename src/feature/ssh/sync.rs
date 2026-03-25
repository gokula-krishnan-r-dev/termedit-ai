use super::client::SshClient;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub struct SshSyncManager {
    pub client: SshClient,
    pub local_cache_dir: PathBuf,
}

impl SshSyncManager {
    pub fn new(client: SshClient) -> Result<Self> {
        let mut dir = std::env::temp_dir();
        dir.push("termedit_ssh_cache");
        dir.push(client.target.replace("@", "_"));
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            client,
            local_cache_dir: dir,
        })
    }

    pub fn to_local_path(&self, remote_path: &Path) -> PathBuf {
        let safe_name = remote_path
            .to_string_lossy()
            .replace("/", "__")
            .replace("\\", "__");
        self.local_cache_dir.join(safe_name)
    }

    pub async fn download_file(&self, remote_path: &Path) -> Result<PathBuf> {
        let local = self.to_local_path(remote_path);
        let mut remote_file = self.client.sftp.open(remote_path).await
            .map_err(|e| anyhow!("Failed to open remote file: {}", e))?;
            
        let mut content = Vec::new();
        loop {
            let chunk = remote_file.read(65536, bytes::BytesMut::new()).await
                .map_err(|e| anyhow!("Failed to read chunk: {}", e))?;
            match chunk {
                Some(data) => content.extend_from_slice(&data),
                None => break,
            }
        }
            
        tokio::fs::write(&local, content).await
            .map_err(|e| anyhow!("Failed to write local cache: {}", e))?;
            
        Ok(local)
    }

    pub async fn upload_file(&self, local_path: &Path, remote_path: &Path) -> Result<()> {
        let content = tokio::fs::read(local_path).await?;
        
        let mut remote_file = self.client.sftp.create(remote_path).await
            .map_err(|e| anyhow!("Failed to create remote file: {}", e))?;
            
        remote_file.write_all(&content).await
            .map_err(|e| anyhow!("Failed to write remote file: {}", e))?;
            
        Ok(())
    }

    pub async fn get_remote_content(&self, remote_path: &Path) -> Result<String> {
        let mut remote_file = self.client.sftp.open(remote_path).await
            .map_err(|e| anyhow!("Failed to open remote file: {}", e))?;
        let mut content = Vec::new();
        loop {
            let chunk = remote_file.read(65536, bytes::BytesMut::new()).await
                .map_err(|e| anyhow!("Failed to read chunk: {}", e))?;
            match chunk {
                Some(data) => content.extend_from_slice(&data),
                None => break,
            }
        }
        let string_content = String::from_utf8(content)
            .map_err(|e| anyhow!("Remote file is not valid UTF-8: {}", e))?;
        Ok(string_content)
    }

    pub fn compute_diff(original: &str, modified: &str) -> String {
        diffy::create_patch(original, modified).to_string()
    }
}
