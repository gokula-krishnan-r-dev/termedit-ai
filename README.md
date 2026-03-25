# TermEdit

A terminal text editor (Ratatui + crossterm) aimed at a familiar **VS Code / Cursor-style** workflow: tabs, mouse, find/replace, **Find in Open Tabs**, **Go to Symbol** (tree-sitter outline), session restore, and optional AI ghost completions. It is **not** a multi-cursor or split-view editor.

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
| `termedit --no-ai` | Disable AI ghost suggestions and the Gemini assistant panel. |
| `termedit --theme <name>` | Theme override (e.g. `tokyo-night`, `dark-plus`). |
| `termedit --config /path/to/config.toml` or `termedit -c /path/to/config.toml` | Use this settings file instead of the default. |
| `termedit --list-gemini-models` | Print built-in Gemini model ids for the **AI assistant** and exit (any valid Google model id also works via config/CLI). |
| `termedit --ai-chat-model <MODEL>` | Use this Gemini model for the assistant panel this session (overrides `ai_chat_model` in config for this run). |
| `termedit --gemini-api-key <KEY>` | Override the API key for this run only; **prefer** `GEMINI_API_KEY` in the environment (avoids shell history). |
| `termedit --print-session` | Print `session.json` as **pretty JSON** to stdout (paths and cursor/scroll metadata only—no buffer text). Exits **1** if missing or invalid. Suitable for **`jq`** and CI. **Privacy:** exposes local file paths. |
| `termedit --session-path` | Print the resolved path to `session.json` and exit **0**; exits **1** if there is no config directory (`dirs::config_dir`). |
| `termedit --open-git-changed` | Runs **`git status --porcelain`** in the **current working directory**, opens each **modified / added / renamed / untracked file** as a tab (skips **deletions** and non-files such as directories). Does **not** restore the saved editor session when used **alone** and git reports **no** files (starts empty instead of restoring). Combine with explicit paths: `termedit a.rs --open-git-changed` opens `a.rs` plus all dirty files. Exits **1** if `git` fails (not a repo, etc.). |

Long flags use **hyphens**: `--no-ai`, `--no-restore`, `--theme`, `--config` (`-c` is a short alias for `--config`). Run `termedit --help` for examples, including files whose names start with `-` (after `--`).

**Config directory** (default): `~/.config/termedit/` — `config.toml` for settings, `session.json` written on exit when session restore is on.

### Automation and git (SSH / servers)

These flags are for **non-interactive** or **scripted** use alongside the TUI—handy on remote machines without a GUI.

```bash
# List paths from the last saved session (requires prior editor run with saved session)
termedit --print-session | jq -r '.paths[]'

# Location of session.json for backup or custom tooling
termedit --session-path

# From inside a git repo: open every dirty file in one editor instance
cd ~/myrepo && termedit --open-git-changed
```

- **`--open-git-changed`** uses your **`git`** binary on `PATH`; there is no embedded git library. Run it from the repository root (or a subdirectory: git still runs against the same repo).
- Session export contains **only** metadata (paths, tab index, line/col, scroll)—not file contents.

---

## Editing model (IDE-style, single cursor)

- **One active buffer** per tab. The tab bar shows every open file; the status bar shows **Tab i/n** when more than one tab is open.
- **Mouse** (when the terminal supports it): click to move the cursor; drag to extend a character-wise selection; wheel to scroll.
- **Paths**: **Open** (`Ctrl+O` / `Cmd+O`) and **Save As** (`Ctrl+Shift+S` / `Cmd+Shift+S`) open a path prompt at the top. You can use `~/project/file.rs`; `~` expands to your home directory.
- **Save** (`Ctrl+S` / `Cmd+S`): writes to the file on disk. Untitled buffers open **Save As** instead.
- **Quit** (`Ctrl+Q`) prompts if any tab is dirty. **Force quit** (`Ctrl+Shift+Q`) exits immediately **without** saving (destructive).
- **Command palette** (`Ctrl+P` / `Cmd+P`): filterable list of common actions (open, save, find, tabs, AI, etc.).

---

## AI assistant (Gemini) and brainstorm

TermEdit can chat with **Google Gemini** inside the editor and insert replies into the buffer. Separate from that, **inline ghost completions** still use the existing Worker-backed API when AI is enabled.

### Setup

1. **API key** (required for the assistant): set environment variable `GEMINI_API_KEY`, *or* add `gemini_api_key = "..."` to `~/.config/termedit/config.toml`. The environment variable is safer for shared machines and scripts.
2. **Model**: optional `ai_chat_model = "gemini-2.5-pro"` in `config.toml`, or CLI `--ai-chat-model` for one session. Run **`termedit --list-gemini-models`** to see built-in ids. You may use **any** valid `models/{id}` name Google documents, even if it is not in that list.
3. **Disable all AI** for a run: `termedit --no-ai` or `ai_enabled = false` in config.

### Opening the assistant

| Action | Shortcut / command |
|--------|----------------------|
| Toggle AI panel | `Ctrl+K` / `Cmd+K` |
| Command palette | `Ctrl/Cmd+P` → **AI Assistant…** or **AI: Brainstorm ideas…** |

### In the panel

- Type a prompt; **Enter** sends (**Shift+Enter** inserts a newline).
- **Tab** (or **Ctrl/Cmd+M**) cycles through the built-in model list. If you started with a **custom** model from config/CLI, Tab moves you through the preset list from the nearest matching slot.
- **PgUp** / **PgDn** and **↑** / **↓** scroll the transcript.
- **Ctrl/Cmd+Shift+I** inserts the **last assistant message** at the cursor (replaces selection if any).
- **Esc** or **Ctrl/Cmd+K** again closes the panel; the chosen model id is kept in the session’s settings. Add `ai_chat_model` to `config.toml` if you want the same default on the next launch.

### Brainstorm ideas (guided prompt)

- **Shortcut**: `Ctrl+Shift+U` / `Cmd+Shift+U`
- **Command palette**: **AI: Brainstorm ideas…**

This opens the assistant with a **prefilled** message that asks for five numbered feature/improvement ideas using the **current file name** and **detected language**. Edit the last line to add a focus (e.g. “performance”) before pressing Enter.

### Configuration (`~/.config/termedit/config.toml`)

```toml
# Master switch for AI (ghost completions + Gemini panel + brainstorm)
ai_enabled = true

# Default Gemini model for the assistant panel (optional)
ai_chat_model = "gemini-2.5-flash"

# Optional API key if not using GEMINI_API_KEY (prefer env in production)
# gemini_api_key = "YOUR_KEY"
```

### CLI examples

```bash
# Discover model names
termedit --list-gemini-models

# One session with a specific model
termedit --ai-chat-model gemini-2.5-pro README.md

# Prefer env for secrets
export GEMINI_API_KEY=your_key
termedit
```

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

## Find in Open Tabs

Search **all open editor tabs** for the same query (like “search in open editors” in full IDEs). This does **not** walk the filesystem—only buffers you already have open.

- **Shortcut**: `Ctrl+Shift+F` / `Cmd+Shift+F`.
- **Command palette**: **Find in Open Tabs…** (`Ctrl/Cmd+P` → type “open tabs” or “find in”).
- **Behavior**: Type a query after `>`; results refresh after a short **debounce** (default **250 ms**). Each row shows **file name**, **line** (1-based), and a **one-line preview**. **↑** / **↓** move the selection; **Enter** switches to that tab and places the cursor on the match; **Esc** closes. Invalid **regex** patterns (when regex mode is on in config) show an error line instead of results.
- **Limits**: At most **`find_in_open_tabs_max_results`** hits are listed (default **500**). Tabs whose buffer is longer than **`find_in_open_tabs_max_chars_per_tab`** characters are **skipped**; the footer reports how many were skipped.

### Configuration (`~/.config/termedit/config.toml`)

```toml
# Enable Find in Open Tabs (default: true)
find_in_open_tabs_enabled = true

# Max hits listed across all tabs (default: 500)
find_in_open_tabs_max_results = 500

# Skip searching a tab if its buffer has more characters than this (default: 2000000)
find_in_open_tabs_max_chars_per_tab = 2000000

# Milliseconds to wait after typing before re-running the search (default: 250)
find_in_open_tabs_debounce_ms = 250

# Match options (defaults: case-insensitive literal substring)
find_in_open_tabs_case_sensitive = false
find_in_open_tabs_whole_word = false
find_in_open_tabs_regex = false
```

Example: case-sensitive **regex** across open tabs:

```toml
find_in_open_tabs_case_sensitive = true
find_in_open_tabs_regex = true
```

---

## Go to Symbol (outline)

Jump to definitions in the **active tab** with a filterable list (same idea as “Goto Symbol in Editor” in VS Code).

- **Shortcut**: `Ctrl+Shift+O` / `Cmd+Shift+O`.
- **Command palette**: **Go to Symbol…** (`Ctrl/Cmd+P` → type “symbol”).
- **Behavior**: Parses the buffer when you open the picker (not on every keystroke). Type to filter by name or kind prefix (`fn`, `struct`, `class`, …). **Enter** jumps to the symbol; **Esc** closes.
- **Supported languages** (matches detected language / extension): **Rust**, **Python**, **JavaScript**, **TypeScript** (TSX grammar, including most `.ts` / `.tsx`), **Go**. Plain text and other languages show an empty list with a short hint.
- **Large files**: If the buffer’s UTF-8 size exceeds `outline_max_bytes` (default **2 000 000**), outline parsing is skipped and a message explains the limit.

### Configuration (`~/.config/termedit/config.toml`)

```toml
# Enable or disable Go to Symbol (default: true)
outline_enabled = true

# Max UTF-8 bytes to parse for outline (default: 2000000)
outline_max_bytes = 2000000
```

Example: disable the feature entirely:

```toml
outline_enabled = false
```

---

## Bracket matching (highlight + jump)

TermEdit highlights the **matching pair** for `()`, `[]`, and `{}` when the cursor is on either bracket (or immediately after a closing bracket, same idea as VS Code). Brackets inside `//` line comments, `/* … */` block comments, `"` strings (with `\` escapes), and `` ` `` backtick strings are **ignored** so pairs line up with real code structure.

- **Jump to matching bracket**: `Ctrl+Shift+\` or `Cmd+Shift+\` (also **Command palette** → *Go to Matching Bracket*, type “bracket”).
- **Highlight**: Shown for both ends of the pair while the cursor sits on a resolvable bracket. Disabled when `bracket_matching = false` or when the buffer is larger than `bracket_match_max_chars` (no highlight and jump shows a short status message).

Angle brackets `<` `>` are not paired (they collide with operators and generics across languages).

### Configuration (`~/.config/termedit/config.toml`)

```toml
# Highlight matching bracket pair (default: true)
bracket_matching = true

# Max characters in the buffer to scan for pairing (default: 2000000).
# Larger files skip bracket highlight and jump-to-bracket.
bracket_match_max_chars = 2000000
```

**Theme (optional)**: override highlight color in a custom theme TOML:

```toml
[ui]
bracket_match_bg = "#374F44"
```

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
| AI assistant panel | `Ctrl/Cmd+K` |
| AI: brainstorm ideas (prefilled prompt) | `Ctrl/Cmd+Shift+U` |
| Insert last AI reply into buffer | `Ctrl/Cmd+Shift+I` (with AI panel open) |

### Movement / selection

Arrows; `Shift+` arrows extend selection; `Ctrl/Cmd+Left/Right` by word; `Ctrl+Home/End` file start/end; `Home`/`End`; `PgUp`/`PgDn`; `Ctrl+Shift+Left/Right` select by word.

### Search

| Action | Shortcut |
|--------|----------|
| Find | `Ctrl/Cmd+F` |
| Replace | `Ctrl/Cmd+H` |
| Find in Open Tabs | `Ctrl/Cmd+Shift+F` |
| Go to line | `Ctrl/Cmd+G` |
| Go to Symbol (outline) | `Ctrl/Cmd+Shift+O` |
| Go to matching bracket | `Ctrl/Cmd+Shift+\` |
| Find next / prev | `F3` / `Shift+F3` |
| Clear search | `Esc` (when no modal) |

---

## Features (summary)

- **Session restore** — reopen tabs and view state; disable with `--no-restore` or `session_restore = false` in config.
- **CLI session export** — `--print-session` / `--session-path` for scripts and `jq` (paths only, no buffer text).
- **Open git-changed files** — `--open-git-changed` opens dirty/untracked files from `git status --porcelain` in cwd.
- **Find in Open Tabs** — search all open tabs with debounced query; `find_in_open_tabs_*` settings (limits, case, whole word, regex).
- **Go to Symbol** — tree-sitter outline for Rust, Python, JS, TS, Go; configurable via `outline_enabled` / `outline_max_bytes`.
- **Bracket matching** — visual pair highlight and jump between `()`, `[]`, `{}`; respects comments/strings; `bracket_matching` / `bracket_match_max_chars` / optional `ui.bracket_match_bg` in themes.
- **AI ghost text** — contextual inline suggestions when enabled; **Tab** to insert.
- **Gemini AI assistant** — conversational panel (`Ctrl/Cmd+K`), model picker, insert reply (`Ctrl/Cmd+Shift+I`); CLI `--list-gemini-models`, `--ai-chat-model`, `--gemini-api-key`; config `ai_chat_model` / `gemini_api_key`.
- **AI brainstorm** — `Ctrl/Cmd+Shift+U` or palette **AI: Brainstorm ideas…** opens the panel with a structured “five ideas” prompt for the current file.
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
