//! Smart Log Explorer — module entry point and `run()` function.

pub mod aggregation;
pub mod ai;
pub mod anomaly;
pub mod config;
pub mod parser;
pub mod query;
pub mod streamer;
pub mod ui;

use std::path::PathBuf;

pub struct LogsOptions {
    /// Path to the log file.
    pub file: String,
    /// Gemini API key (optional — AI features require it).
    pub api_key: Option<String>,
    /// Model id for AI queries.
    pub model_id: String,
    /// Start from the beginning of the file instead of the tail.
    pub from_start: bool,
}

/// Entry point called from `main.rs`.
pub fn run(opts: LogsOptions) -> anyhow::Result<()> {
    let path = PathBuf::from(&opts.file);
    if !path.exists() {
        anyhow::bail!("log file not found: {}", opts.file);
    }

    let cfg = config::LogsConfig::load();

    ui::run_tui(path, opts.from_start, opts.api_key, opts.model_id, cfg)
}
