use anyhow::{anyhow, Result};
use openssh::{Session, SessionBuilder};
use openssh_sftp_client::Sftp;
use std::sync::Arc;

#[derive(Clone)]
pub struct SshClient {
    pub target: String,
    pub session: Arc<Session>,
    pub sftp: Arc<Sftp>,
}

impl SshClient {
    pub async fn connect(target: &str) -> Result<Self> {
        let session = SessionBuilder::default()
            .connect(target)
            .await
            .map_err(|e| anyhow!("SSH Connect failed: {}", e))?;

        let mut child = session
            .subsystem("sftp")
            .stdin(openssh::Stdio::piped())
            .stdout(openssh::Stdio::piped())
            .spawn()
            .await
            .map_err(|e| anyhow!("Failed to spawn SFTP subsystem: {}", e))?;

        let sftp = Sftp::new(
            child.stdin().take().unwrap(),
            child.stdout().take().unwrap(),
            Default::default(),
        )
        .await
        .map_err(|e| anyhow!("Failed to initialize SFTP: {}", e))?;

        Ok(Self {
            target: target.to_string(),
            session: Arc::new(session),
            sftp: Arc::new(sftp),
        })
    }
}
