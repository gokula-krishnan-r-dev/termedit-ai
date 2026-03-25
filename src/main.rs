/// TermEdit — A modern, AI-powered terminal text editor.
///
/// Entry point: parses CLI arguments, loads config/theme, and starts the app.
/// Supports session restore: when no files are passed, restores last open files and cursor/scroll.

mod app;
mod config;
mod core;
mod error;
mod feature;
mod ui;

use std::collections::HashSet;
use std::path::Path;

use clap::Parser;
#[cfg(feature = "ai")]
use feature::gemini_chat;
use feature::git_worktree;
use feature::session;

const AFTER_LONG_HELP: &str = "\
Examples:
  termedit
      Restore the last session, or start with one empty buffer if none is saved.

  termedit file.py src/main.rs
      Open the given files as tabs (paths may be new; session is not restored).

  termedit --no-restore
      Start fresh: do not load session.json; leaves any existing snapshot for the next normal launch.

  termedit --theme tokyo-night --no-ai
      Use a specific theme and disable AI for this run.

  termedit --list-gemini-models
      Print Gemini model ids for the AI assistant and exit.

  termedit --ai-chat-model gemini-2.5-pro
      Use this Gemini model for the in-editor AI panel for this session.

  termedit --print-session
      Print session.json as pretty JSON (for scripts); lists saved file paths only.

  termedit --session-path
      Print path to session.json.

  termedit --open-git-changed
      Open git dirty/untracked files from the current repo as tabs.

  termedit --ai-server
      (Feature `ai-server`) Interactive DevOps assistant: server context + Gemini REPL.

  termedit -- --weird-name.txt
      Open a file whose name starts with '-' (paths after -- are files).
";

/// Terminal text editor with tabs, session restore, and optional AI.
#[derive(Parser, Debug)]
#[command(
    name = "termedit",
    version,
    about = "Terminal text editor with tabs, session restore, and optional AI completions.",
    after_long_help = AFTER_LONG_HELP
)]
struct Cli {
    /// Files to open as tabs (session is not restored when any are given).
    #[arg(value_name = "FILE")]
    files: Vec<String>,

    /// Theme name: built-ins include dark-plus, one-dark-pro, catppuccin-mocha, tokyo-night, or a name matching ~/.config/termedit/themes/<name>.toml.
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,

    /// Disable AI features for this run.
    #[arg(long)]
    no_ai: bool,

    /// Do not restore or overwrite session data for this run.
    #[arg(long)]
    no_restore: bool,

    /// Use this config file instead of the default.
    #[arg(short = 'c', long, value_name = "PATH")]
    config: Option<String>,

    /// Gemini model id for the AI assistant panel (see `termedit --list-gemini-models`). Any valid API id works.
    #[arg(long, value_name = "MODEL")]
    ai_chat_model: Option<String>,

    /// Print Gemini assistant model ids and key hints, then exit.
    #[arg(long)]
    list_gemini_models: bool,

    /// Override Gemini API key for this run only (shell history may retain it; prefer GEMINI_API_KEY).
    #[arg(long, value_name = "KEY")]
    gemini_api_key: Option<String>,

    /// Print saved session as pretty JSON to stdout (exposes local file paths). Exits 1 if missing/invalid.
    #[arg(long, conflicts_with = "session_path")]
    print_session: bool,

    /// Print the path to session.json and exit (0). Exits 1 if config directory is unavailable.
    #[arg(long, conflicts_with = "print_session")]
    session_path: bool,

    /// Open files reported by `git status --porcelain` in the current directory as tabs (skips session restore if any paths listed; skips deletions).
    #[arg(long)]
    open_git_changed: bool,

    /// AI DevOps assistant: collect logs, configs, metrics; Gemini REPL (requires `ai-server` feature).
    #[arg(long = "ai-server")]
    ai_server: bool,

    /// Gemini model id for `--ai-server` (overrides `--ai-chat-model` / config when set).
    #[arg(long = "ai-server-model", value_name = "MODEL")]
    ai_server_model: Option<String>,

    /// Override cache directory for `--ai-server` context snapshots.
    #[arg(long = "ai-server-cache-dir", value_name = "DIR")]
    ai_server_cache_dir: Option<std::path::PathBuf>,

    /// Show diffs and prompts but do not write files in `--ai-server` mode.
    #[arg(long = "ai-server-dry-run")]
    ai_server_dry_run: bool,

    /// Send literal `.env` values to the model (unsafe; default is REDACTED).
    #[arg(long = "ai-server-include-secrets")]
    ai_server_include_secrets: bool,
}

fn main() -> anyhow::Result<()> {
    // Initialize logger
    env_logger::init();

    // Parse CLI arguments
    let cli = Cli::parse();

    if cli.ai_server {
        #[cfg(feature = "ai-server")]
        {
            let settings = config::settings::Settings::load();
            let api_some = feature::ai_server::resolve_api_key(
                cli.gemini_api_key.as_deref(),
                settings.gemini_api_key.as_deref(),
            );
            let Some(api_key) = api_some else {
                anyhow::bail!(
                    "termedit --ai-server requires GEMINI_API_KEY, or --gemini-api-key, or gemini_api_key in config."
                );
            };
            let model = feature::ai_server::resolve_model(
                cli.ai_server_model
                    .as_deref()
                    .or(cli.ai_chat_model.as_deref()),
                settings.ai_chat_model.as_deref(),
            );
            return feature::ai_server::run(feature::ai_server::AiServerOptions {
                api_key,
                model_id: model,
                cache_dir: cli.ai_server_cache_dir.clone(),
                dry_run: cli.ai_server_dry_run,
                include_secrets: cli.ai_server_include_secrets,
            });
        }
        #[cfg(not(feature = "ai-server"))]
        {
            eprintln!(
                "termedit: --ai-server requires building with feature `ai-server` (e.g. cargo install termedit --features ai-server)."
            );
            return Ok(());
        }
    }

    if cli.list_gemini_models {
        #[cfg(feature = "ai")]
        {
            print!("{}", gemini_chat::models_list_text());
            return Ok(());
        }
        #[cfg(not(feature = "ai"))]
        {
            eprintln!(
                "termedit: this build was compiled without AI. Use: cargo install termedit --features ai"
            );
            return Ok(());
        }
    }

    if cli.print_session {
        let Some(path) = session::default_session_path() else {
            eprintln!("termedit: no config directory; cannot locate session.json");
            std::process::exit(1);
        };
        let Some(state) = session::SessionState::load_from(&path) else {
            eprintln!(
                "termedit: no valid session at {}",
                path.display()
            );
            std::process::exit(1);
        };
        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| anyhow::anyhow!("serialize session: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    if cli.session_path {
        let Some(path) = session::default_session_path() else {
            eprintln!("termedit: no config directory");
            std::process::exit(1);
        };
        println!("{}", path.display());
        return Ok(());
    }

    // Load settings
    let mut settings = if let Some(ref config_path) = cli.config {
        config::settings::Settings::from_file(Path::new(config_path))
            .unwrap_or_else(|_| config::settings::Settings::default())
    } else {
        config::settings::Settings::load()
    };

    // Apply CLI overrides
    settings.merge_cli(
        cli.theme.as_deref(),
        cli.no_ai,
        cli.no_restore,
        cli.ai_chat_model.as_deref(),
        cli.gemini_api_key.as_deref(),
    );

    // Load theme
    let theme = config::theme::Theme::load(&settings.theme);

    let mut file_list: Vec<String> = cli.files.clone();
    if cli.open_git_changed {
        let cwd = std::env::current_dir().map_err(|e| anyhow::anyhow!("current_dir: {}", e))?;
        let paths = match git_worktree::changed_file_paths(&cwd) {
            Ok(p) => p,
            Err(msg) => {
                eprintln!("termedit: {}", msg);
                std::process::exit(1);
            }
        };
        for p in paths {
            if let Some(s) = p.to_str() {
                file_list.push(s.to_string());
            }
        }
    }

    file_list = dedupe_preserve_order(file_list);

    if cli.open_git_changed && cli.files.is_empty() && file_list.is_empty() {
        eprintln!(
            "termedit: no git-changed files to open (clean tree, only deletions, or paths are not files)."
        );
    }

    let git_only_empty = cli.open_git_changed && cli.files.is_empty() && file_list.is_empty();
    let do_restore =
        file_list.is_empty() && settings.session_restore && !git_only_empty;

    // Create app
    let mut app = app::App::new(settings, theme);

    if do_restore {
        if let Some(session_path) = session::default_session_path() {
            if let Some(state) = session::SessionState::load_from(&session_path) {
                app.restore_from_session(&state);
            }
        }
    } else {
        for file_path in &file_list {
            let path = Path::new(file_path);
            if let Err(e) = app.open_file(path) {
                eprintln!("Warning: could not open '{}': {}", file_path, e);
            }
        }
        app.drop_cli_placeholder_tab_if_redundant();
    }

    // Run the editor
    app.run()?;

    Ok(())
}

fn dedupe_preserve_order(paths: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        if seen.insert(p.clone()) {
            out.push(p);
        }
    }
    out
}
