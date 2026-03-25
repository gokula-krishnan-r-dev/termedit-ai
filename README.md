# TermEdit

A modern, nano-style terminal text editor with a focus on developer productivity, clean architecture, and lightweight performance.

## Features

- **Session restore** — Restart the editor with no file arguments and return to your last session: same open files, active tab, cursor position, and scroll. No other nano-style TUI editor does this out of the box. Disable with `--no-restore` or `session_restore = false` in config.
- **AI ghost text** — Inline code suggestions (e.g. after `SELECT ` → `* FROM users;`). Tab to accept. Local, pattern-based completion; extensible for API-backed AI.
- **Professional shortcuts** — Ctrl/Cmd+S save, Ctrl/Cmd+W close (with save confirmation), Ctrl/Cmd+Delete or Backspace delete line, Ctrl+Tab / Ctrl+Shift+Tab switch tabs.
- **Syntax highlighting** — Tree-sitter for Rust, Python, JavaScript, TypeScript, Go.
- **Search** — Find (Ctrl+F), replace (Ctrl+H), go to line (Ctrl+G).
- **Multiple tabs** — Open several files; close with confirmation when modified.
- **Undo/redo** — Full history with coalescing.
- **Themes** — dark-plus, one-dark-pro, catppuccin-mocha, tokyo-night, and more via config.

## Usage

```bash
termedit                    # Restore last session (or new empty buffer)
termedit file.py src/main.rs
termedit --no-restore       # Start fresh, no session restore
termedit --theme tokyo-night --no-ai
```

## Config

Settings and theme: `~/.config/termedit/config.toml`  
Session file: `~/.config/termedit/session.json` (created on exit when session restore is enabled).

## Build

```bash
cargo build --release
```

Optimized for size and speed (LTO, strip, single codegen unit in release).
# termedit-ai
