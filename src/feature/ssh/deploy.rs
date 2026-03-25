use super::client::SshClient;
use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub struct SshDeployer {
    pub client: SshClient,
}

impl SshDeployer {
    pub fn new(client: SshClient) -> Self {
        Self { client }
    }

    pub fn config_path(target: &str) -> Option<PathBuf> {
        let mut path = dirs::config_dir()?;
        path.push("termedit");
        path.push("deploy");
        let safe_target = target.replace("@", "_");
        path.push(format!("{}.toml", safe_target));
        Some(path)
    }

    pub fn load_deploy_cmd(target: &str) -> Result<String> {
        let path = Self::config_path(target)
            .ok_or_else(|| anyhow!("Could not determine config directory"))?;
            
        if !path.exists() {
            return Err(anyhow!("No deploy config found for {}. Create one at {:?}", target, path));
        }
        
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| anyhow!("Failed to read config {:?}: {}", path, e))?;
            
        let value: toml::Value = toml::from_str(&contents)
            .map_err(|e| anyhow!("Failed to parse config {:?}: {}", path, e))?;
            
        let cmd = value
            .get("deploy")
            .and_then(|d| d.get("cmd"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow!("Deploy config {:?} must contain [deploy]\\ncmd = \"...\"", path))?;
            
        Ok(cmd.to_string())
    }

    pub async fn execute_deploy(&self) -> Result<String> {
        let cmd = Self::load_deploy_cmd(&self.client.target)?;
        
        let output = self.client.session.command("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .await
            .map_err(|e| anyhow!("Failed to execute remote command: {}", e))?;
            
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        
        if output.status.success() {
            Ok(format!("Deploy successful:\n{}", stdout))
        } else {
            Err(anyhow!("Deploy failed ({}):\n{}\n{}", output.status, stderr, stdout))
        }
    }
}
