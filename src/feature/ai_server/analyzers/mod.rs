//! Pluggable analyzers (compile-time registry).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::feature::ai_server::collect;
use crate::feature::ai_server::context::{CollectConfig, ServerContext};

#[async_trait]
pub trait Analyzer: Send + Sync {
    fn id(&self) -> &'static str;
    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()>;
}

pub struct MetricsAnalyzer;

#[async_trait]
impl Analyzer for MetricsAnalyzer {
    fn id(&self) -> &'static str {
        "metrics"
    }

    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
        collect::metrics::contribute(ctx, config).await
    }
}

pub struct LogsAnalyzer;

#[async_trait]
impl Analyzer for LogsAnalyzer {
    fn id(&self) -> &'static str {
        "logs"
    }

    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
        collect::logs::contribute(ctx, config).await
    }
}

pub struct DotenvAnalyzer;

#[async_trait]
impl Analyzer for DotenvAnalyzer {
    fn id(&self) -> &'static str {
        "dotenv"
    }

    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
        collect::dotenv::contribute(ctx, config).await
    }
}

pub struct NginxAnalyzer;

#[async_trait]
impl Analyzer for NginxAnalyzer {
    fn id(&self) -> &'static str {
        "nginx"
    }

    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
        collect::nginx::contribute(ctx, config).await
    }
}

pub struct DockerAnalyzer;

#[async_trait]
impl Analyzer for DockerAnalyzer {
    fn id(&self) -> &'static str {
        "docker"
    }

    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
        collect::docker::contribute(ctx, config).await
    }
}

pub struct SystemdAnalyzer;

#[async_trait]
impl Analyzer for SystemdAnalyzer {
    fn id(&self) -> &'static str {
        "systemd"
    }

    async fn contribute(&self, ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
        collect::systemd::contribute(ctx, config).await
    }
}

pub fn default_registry() -> Vec<Arc<dyn Analyzer>> {
    vec![
        Arc::new(MetricsAnalyzer),
        Arc::new(LogsAnalyzer),
        Arc::new(DotenvAnalyzer),
        Arc::new(NginxAnalyzer),
        Arc::new(DockerAnalyzer),
        Arc::new(SystemdAnalyzer),
    ]
}

pub async fn run_all(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    for a in default_registry() {
        log::debug!("ai-server analyzer: {}", a.id());
        a.contribute(ctx, config).await?;
    }
    Ok(())
}
