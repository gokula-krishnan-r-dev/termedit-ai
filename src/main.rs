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

use std::path::Path;

use clap::Parser;
use feature::session;

/// TermEdit — A modern terminal text editor with AI assistance and session restore.
#[derive(Parser, Debug)]
#[command(name = "termedit", version, about)]
struct Cli {
    /// Files to open.
    #[arg()]
    files: Vec<String>,

    /// Theme name (e.g., dark-plus, one-dark-pro, catppuccin-mocha).
    #[arg(long, default_value = None)]
    theme: Option<String>,

    /// Disable AI features.
    #[arg(long)]
    no_ai: bool,

    /// Do not restore previous session (open files, cursor, scroll).
    #[arg(long)]
    no_restore: bool,

    /// Custom config file path.
    #[arg(long)]
    config: Option<String>,
}

fn main() -> anyhow::Result<()> {
    // Initialize logger
    env_logger::init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Load settings
    let mut settings = if let Some(ref config_path) = cli.config {
        config::settings::Settings::from_file(Path::new(config_path))
            .unwrap_or_else(|_| config::settings::Settings::default())
    } else {
        config::settings::Settings::load()
    };

    // Apply CLI overrides
    settings.merge_cli(cli.theme.as_deref(), cli.no_ai, cli.no_restore);

    // Load theme
    let theme = config::theme::Theme::load(&settings.theme);

    let do_restore = cli.files.is_empty() && settings.session_restore;

    // Create app
    let mut app = app::App::new(settings, theme);

    if do_restore {
        if let Some(session_path) = session::default_session_path() {
            if let Some(state) = session::SessionState::load_from(&session_path) {
                app.restore_from_session(&state);
            }
        }
    } else {
        for file_path in &cli.files {
            let path = Path::new(file_path);
            if let Err(e) = app.open_file(path) {
                eprintln!("Warning: could not open '{}': {}", file_path, e);
            }
        }
    }

    // Run the editor
    app.run()?;

    Ok(())
}
