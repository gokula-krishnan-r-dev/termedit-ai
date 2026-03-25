# TermEdit

A terminal text editor (Ratatui + crossterm) aimed at a familiar **VS Code / Cursor-style** workflow: tabs, mouse, find/replace, session restore, and optional AI ghost completions. It is **not** a multi-cursor or split-view editor.

---

## Quick start

```bash
cargo build --release
# Binary: target/release/termedit
termedit
termedit README.md src/main.rs
```

---

## CLI behavior

| Invocation | Effect |
|------------|--------|
| `termedit` | If **session restore** is enabled (default), reopen files, active tab, and cursor/scroll from `~/.config/termedit/session.json`. Otherwise start with one empty buffer. |
| `termedit a b c` | Open **multiple tabs** in order (paths need not exist yet; missing paths get an empty buffer tied to that path). |
| `termedit --no-restore` | Ignore the saved session for this run. |
| `termedit --no-ai` | Disable AI ghost suggestions. |
| `termedit --theme <name>` | Theme override (e.g. `tokyo-night`, `dark-plus`). |
| `termedit --config /path/to/config.toml` | Use this settings file instead of the default. |

Long flags use **hyphens**: `--no-ai`, `--no-restore`, `--theme`, `--config`.

**Config directory** (default): `~/.config/termedit/` — `config.toml` for settings, `session.json` written on exit when session restore is on.

---

## Editing model (IDE-style, single cursor)

- **One active buffer** per tab. The tab bar shows every open file; the status bar shows **Tab i/n** when more than one tab is open.
- **Mouse** (when the terminal supports it): click to move the cursor; drag to extend a character-wise selection; wheel to scroll.
- **Paths**: **Open** (`Ctrl+O` / `Cmd+O`) and **Save As** (`Ctrl+Shift+S` / `Cmd+Shift+S`) open a path prompt at the top. You can use `~/project/file.rs`; `~` expands to your home directory.
- **Save** (`Ctrl+S` / `Cmd+S`): writes to the file on disk. Untitled buffers open **Save As** instead.
- **Quit** (`Ctrl+Q`) prompts if any tab is dirty. **Force quit** (`Ctrl+Shift+Q`) exits immediately **without** saving (destructive).
- **Command palette** (`Ctrl+P` / `Cmd+P`): filterable list of common actions (open, save, find, tabs, etc.).

---

## Search and replace

- **Find** (`Ctrl+F` / `Cmd+F`): modal with live search; **Enter** applies and closes; **Esc** clears search highlights when no modal is open.
- **Find / replace** (`Ctrl+H` / `Cmd+H`): two fields. **Tab** / **Shift+Tab** switches between Find and Replace.
  - Focus **Find**, **Enter**: run search (same as plain Find).
  - Focus **Replace**, **Enter**: replace the **current** match, refresh search, keep the modal open; jumps to the next match when possible.
  - Focus **Replace**, **Ctrl+Enter**: **Replace all** matches, then close the modal.
- **Next / previous match** after a search: **F3** / **Shift+F3** (also when not in a modal, if matches exist).
- **Go to line** (`Ctrl+G` / `Cmd+G`): line number (1-based).

---

## Keyboard shortcuts

**macOS:** most `Ctrl` shortcuts also work with **`Cmd` (`Super`)** where noted.

### File / tabs / app

| Action | Shortcut |
|--------|----------|
| Save | `Ctrl/Cmd+S` |
| Save As (path prompt) | `Ctrl/Cmd+Shift+S` |
| Open (path prompt) | `Ctrl/Cmd+O` |
| New file | `Ctrl/Cmd+N` |
| Close tab | `Ctrl/Cmd+W` |
| Quit (confirm if dirty) | `Ctrl+Q` |
| Force quit (no save) | `Ctrl+Shift+Q` |
| Next / previous tab | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Next / previous tab | `Ctrl+PageDown` / `Ctrl+PageUp` |
| Jump to tab 1–9 | `Alt+1` … `Alt+9` (tabs beyond count clamp to last) |
| Command palette | `Ctrl/Cmd+P` |
| Toggle file tree | `Ctrl/Cmd+B` |

### Editing

| Action | Shortcut |
|--------|----------|
| Undo / redo | `Ctrl/Cmd+Z`, `Ctrl/Cmd+Shift+Z` or `Ctrl/Cmd+Y` |
| Cut / copy / paste | `Ctrl/Cmd+X`, `Ctrl/Cmd+C`, `Ctrl/Cmd+V` |
| Select all | `Ctrl/Cmd+A` |
| Select line | `Ctrl/Cmd+L` |
| Delete line | `Ctrl/Cmd+D` or `Ctrl/Cmd+Delete` / `Cmd+Backspace` |
| Duplicate line | `Ctrl/Cmd+Shift+D` |
| Toggle comment | `Ctrl/Cmd+/` |
| Indent / dedent | `Tab` / `Shift+Tab` (selection or line) |
| Move line | `Alt+Up` / `Alt+Down` |
| Accept AI ghost / completion | `Tab` (when suggested) |

### Movement / selection

Arrows; `Shift+` arrows extend selection; `Ctrl/Cmd+Left/Right` by word; `Ctrl+Home/End` file start/end; `Home`/`End`; `PgUp`/`PgDn`; `Ctrl+Shift+Left/Right` select by word.

### Search

| Action | Shortcut |
|--------|----------|
| Find | `Ctrl/Cmd+F` |
| Replace | `Ctrl/Cmd+H` |
| Go to line | `Ctrl/Cmd+G` |
| Find next / prev | `F3` / `Shift+F3` |
| Clear search | `Esc` (when no modal) |

### Reserved / limited

- **`Ctrl/Cmd+K`**: reserved for a future AI panel (no UI yet).

---

## Features (summary)

- **Session restore** — reopen tabs and view state; disable with `--no-restore` or `session_restore = false` in config.
- **AI ghost text** — contextual inline suggestions when enabled; **Tab** to insert.
- **Syntax highlighting** — pattern-based highlighter for common languages (see `src/feature/syntax.rs`).
- **Undo / redo** with history.
- **Themes** — built-in names in `src/config/theme.rs`; override via config or `--theme`.

---

## Build

```bash
cargo build --release
```

Release profile is tuned for size (LTO, strip, single codegen unit).

---

## Contributing / architecture

See [docs/DEVELOPER.md](docs/DEVELOPER.md) for module layout, save pipeline, modals, and extending the command palette.
