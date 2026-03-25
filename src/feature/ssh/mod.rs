pub mod client;
pub mod deploy;
pub mod sync;
pub mod ui;

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::runtime::Runtime;

pub struct SshContext {
    pub sync: sync::SshSyncManager,
    pub deployer: deploy::SshDeployer,
    pub rt: Runtime,
    pub local_to_remote: HashMap<PathBuf, PathBuf>,
}

impl SshContext {
    pub fn init(target: &str) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        
        let client = rt.block_on(client::SshClient::connect(target))?;
        let sync = sync::SshSyncManager::new(client.clone())?;
        let deployer = deploy::SshDeployer::new(client);

        Ok(Self {
            sync,
            deployer,
            rt,
            local_to_remote: HashMap::new(),
        })
    }
}
