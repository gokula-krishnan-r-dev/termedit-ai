# TermEdit Testing & Usage Guide

This guide provides instructions on how to run, test, and use the basic features of **TermEdit**.

## 🚀 How to Run & Test

You can test the editor by running it via `cargo`:

```bash
# Run without opening a file
cargo run

# Run and open a specific file
cargo run -- src/main.rs

# Run with a specific theme
cargo run -- --theme catppuccin-mocha src/main.rs
```
demo
If you want to test the release build for better performance:
```bash
cargo run --release
```

## ⌨️ Basic Keyboard Shortcuts

TermEdit uses standard VS Code compatible shortcuts. Here are the most important ones:

### File Operations
* **Save:** `Ctrl + S`
* **Save As:** `Ctrl + Shift + S`
* **Quit / Exit Editor:** `Ctrl + Q`
* **Close Current Tab:** `Ctrl + W`
* **New File:** `Ctrl + N`
* **Open File:** `Ctrl + O`

### Navigation & Search
* **Find:** `Ctrl + F`
* **Find & Replace:** `Ctrl + H`
* **Go to Line:** `Ctrl + G`
* **Toggle File Tree:** `Ctrl + B`

### Editing
* **Undo:** `Ctrl + Z`
* **Redo:** `Ctrl + Y` 
* **Copy:** `Ctrl + C`
* **Cut:** `Ctrl + X`
* **Paste:** `Ctrl + V`
* **Select All:** `Ctrl + A`

## 🚪 How to Save and Exit

1. **To Save:** Press `Ctrl + S`. You should see a "Saved" message in the status bar at the bottom.
2. **To Exit:** Press `Ctrl + Q`.
   * If you have no unsaved changes, the app will exit immediately.
   * If you have unsaved changes, a confirmation prompt will appear at the top. Press `y` to save and exit, `n` to discard changes and exit, or `Esc` to cancel the exit operation.
