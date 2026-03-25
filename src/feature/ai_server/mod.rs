//! AI-powered server context mode (`termedit --ai-server`).
//!
//! Collects logs, configs, metrics; asks Gemini; optionally applies file edits safely.

mod analyzers;
mod apply;
mod cache;
mod collect;
mod context;
mod gemini;
mod prompt;
mod repl;
mod response;
mod summarize;
mod ui;

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

pub use context::{CollectConfig, ServerContext};
pub use gemini::{GeminiLlm, LlmClient, MockLlm};

use crate::feature::gemini_chat::resolve_chat_model_id;

use analyzers::run_all;
use cache::{ContextCache, default_cache_dir};

/// Options assembled from CLI + config (see [`run`]).
pub struct AiServerOptions {
    pub api_key: String,
    pub model_id: String,
    pub cache_dir: Option<PathBuf>,
    pub dry_run: bool,
    pub include_secrets: bool,
}

pub fn run(opts: AiServerOptions) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("tokio runtime")?
        .block_on(run_async(opts))
}

async fn run_async(opts: AiServerOptions) -> Result<()> {
    let llm = GeminiLlm::new(opts.api_key.clone(), opts.model_id.clone())?;
    repl::run_repl(&llm, &opts).await
}

pub(crate) async fn collect_context(opts: &AiServerOptions, force_refresh: bool) -> Result<ServerContext> {
    if opts.include_secrets {
        eprintln!("termedit: WARNING: --ai-server-include-secrets sends literal .env values to the model.");
    }
    let hostname = hostname();
    let cwd = std::env::current_dir()?;
    let mut config = CollectConfig::with_cwd(cwd.clone());
    config.include_secrets = opts.include_secrets;

    let cache_dir = opts.cache_dir.clone().or_else(default_cache_dir);
    let ttl = Duration::from_secs(config.cache_ttl_secs);
    let key = ContextCache::cache_key(&hostname, &config);

    if !force_refresh {
        if let Some(ref dir) = cache_dir {
            let c = ContextCache::new(dir.clone(), ttl);
            if let Some(ctx) = c.get_valid(&key).await? {
                return Ok(ctx);
            }
        }
    }

    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message("Collecting server context…");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let mut ctx = ServerContext::empty(
        hostname.clone(),
        cwd.to_string_lossy().into_owned(),
        vec![],
    );
    run_all(&mut ctx, &config).await?;

    spinner.finish_and_clear();

    if let Some(dir) = cache_dir {
        let c = ContextCache::new(dir, ttl);
        c.put(&key, &ctx).await?;
    }
    Ok(ctx)
}

fn hostname() -> String {
    sysinfo::System::host_name()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Resolve API key from env, CLI, config, then optional embed (see `config::embed`).
pub fn resolve_api_key(from_cli: Option<&str>, from_config: Option<&str>) -> Option<String> {
    std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            from_cli
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            from_config
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            crate::config::embed::embedded_gemini_api_key().map(|s| s.to_string())
        })
}

/// Resolve model id using the same helper as the editor.
pub fn resolve_model(from_cli: Option<&str>, from_config: Option<&str>) -> String {
    resolve_chat_model_id(from_cli.or(from_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collect_smoke_metrics_and_cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let opts = AiServerOptions {
            api_key: "x".to_string(),
            model_id: "gemini-2.5-flash".to_string(),
            cache_dir: Some(dir.path().to_path_buf()),
            dry_run: true,
            include_secrets: false,
        };
        let ctx = collect_context(&opts, true).await.unwrap();
        assert_eq!(ctx.context_version, super::context::CONTEXT_VERSION);
        let ctx2 = collect_context(&opts, false).await.unwrap();
        assert_eq!(ctx2.hostname, ctx.hostname);
    }

    #[tokio::test]
    async fn mock_llm_parse_apply_dry_run() {
        let plan_json = r#"{"explanation":"ok","suggested_fixes":[],"shell_commands":["echo hi"],"file_edits":[]}"#;
        let llm = MockLlm {
            response: plan_json.to_string(),
        };
        let ctx = ServerContext::empty("test-host".into(), "/tmp".into(), vec![]);
        let sys = super::prompt::system_instruction_json_only();
        let user = super::prompt::user_message("status?", &ctx).unwrap();
        let raw = llm.generate_json(&sys, &user).await.unwrap();
        let plan = super::response::AssistantPlan::parse_model_text(&raw).unwrap();
        super::apply::offer_apply_plan(&plan, true).unwrap();
    }
}
