/// Keyboard shortcut definitions — VS Code compatible key bindings.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// An editor action triggered by a key binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // === Cursor Movement ===
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    WordLeft,
    WordRight,
    Home,
    End,
    FileStart,
    FileEnd,
    PageUp,
    PageDown,
    GoToLine,

    // === Selection ===
    SelectLeft,
    SelectRight,
    SelectUp,
    SelectDown,
    SelectWordLeft,
    SelectWordRight,
    SelectHome,
    SelectEnd,
    SelectAll,
    SelectLine,

    // === Editing ===
    InsertChar(char),
    InsertNewline,
    InsertTab,
    Backspace,
    Delete,
    Undo,
    Redo,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    ToggleComment,
    Indent,
    Dedent,

    // === Clipboard ===
    Copy,
    Cut,
    Paste,

    // === File Operations ===
    Save,
    SaveAs,
    OpenFile,
    NewFile,
    CloseBuffer,

    // === Search ===
    Find,
    FindReplace,
    FindNext,
    FindPrev,
    EscapeSearch,

    // === View ===
    ToggleFileTree,
    ToggleAiPanel,
    CommandPalette,

    // === Tabs ===
    NextTab,
    PrevTab,

    // === App ===
    Quit,
    ForceQuit,

    // === Mouse ===
    MouseClick(u16, u16),
    MouseDrag(u16, u16),
    MouseScroll(i16),

    // === No-op ===
    None,
}

/// Map a crossterm KeyEvent to an editor Action.
pub fn map_key_event(event: KeyEvent) -> Action {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let super_ = event.modifiers.contains(KeyModifiers::SUPER);

    match event.code {
        // === Movement ===
        KeyCode::Left if ctrl && shift => Action::SelectWordLeft,
        KeyCode::Right if ctrl && shift => Action::SelectWordRight,
        KeyCode::Left if ctrl => Action::WordLeft,
        KeyCode::Right if ctrl => Action::WordRight,
        KeyCode::Left if shift => Action::SelectLeft,
        KeyCode::Right if shift => Action::SelectRight,
        KeyCode::Up if shift => Action::SelectUp,
        KeyCode::Down if shift => Action::SelectDown,
        KeyCode::Up if alt => Action::MoveLineUp,
        KeyCode::Down if alt => Action::MoveLineDown,
        KeyCode::Left => Action::MoveLeft,
        KeyCode::Right => Action::MoveRight,
        KeyCode::Up => Action::MoveUp,
        KeyCode::Down => Action::MoveDown,
        KeyCode::Home if ctrl => Action::FileStart,
        KeyCode::End if ctrl => Action::FileEnd,
        KeyCode::Home if shift => Action::SelectHome,
        KeyCode::End if shift => Action::SelectEnd,
        KeyCode::Home => Action::Home,
        KeyCode::End => Action::End,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,

        // === Ctrl/Cmd shortcuts ===
        KeyCode::Char('q') if ctrl => Action::Quit,
        KeyCode::Char('s') if (ctrl || super_) && shift => Action::SaveAs,
        KeyCode::Char('s') if ctrl || super_ => Action::Save,
        KeyCode::Char('z') if ctrl => Action::Undo,
        KeyCode::Char('y') if ctrl => Action::Redo,
        KeyCode::Char('c') if ctrl => Action::Copy,
        KeyCode::Char('x') if ctrl => Action::Cut,
        KeyCode::Char('v') if ctrl => Action::Paste,
        KeyCode::Char('a') if ctrl => Action::SelectAll,
        KeyCode::Char('l') if ctrl => Action::SelectLine,
        KeyCode::Char('d') if ctrl => Action::DeleteLine,
        KeyCode::Char('f') if ctrl => Action::Find,
        KeyCode::Char('h') if ctrl => Action::FindReplace,
        KeyCode::Char('g') if ctrl => Action::GoToLine,
        KeyCode::Char('n') if ctrl => Action::NewFile,
        KeyCode::Char('o') if ctrl => Action::OpenFile,
        KeyCode::Char('w') if ctrl || super_ => Action::CloseBuffer,
        KeyCode::Char('b') if ctrl => Action::ToggleFileTree,
        KeyCode::Char('k') if ctrl => Action::ToggleAiPanel,
        KeyCode::Char('p') if ctrl => Action::CommandPalette,
        KeyCode::Char('/') if ctrl => Action::ToggleComment,
        KeyCode::Tab if ctrl && shift => Action::PrevTab,
        KeyCode::Tab if ctrl => Action::NextTab,
        KeyCode::Tab if shift => Action::Dedent,

        // === Basic editing (Super/Ctrl+Backspace/Delete before plain) ===
        KeyCode::Backspace if super_ => Action::DeleteLine,
        KeyCode::Delete if super_ => Action::DeleteLine,
        KeyCode::Delete if ctrl => Action::DeleteLine,
        KeyCode::Char(c) => Action::InsertChar(c),
        KeyCode::Enter => Action::InsertNewline,
        KeyCode::Tab => Action::InsertTab,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Delete => Action::Delete,
        KeyCode::Esc => Action::EscapeSearch,

        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_char() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(map_key_event(event), Action::InsertChar('a'));
    }

    #[test]
    fn test_ctrl_s() {
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(map_key_event(event), Action::Save);
    }

    #[test]
    fn test_ctrl_z() {
        let event = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(map_key_event(event), Action::Undo);
    }

    #[test]
    fn test_shift_arrows() {
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT);
        assert_eq!(map_key_event(event), Action::SelectLeft);
    }

    #[test]
    fn test_ctrl_left() {
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(map_key_event(event), Action::WordLeft);
    }

    #[test]
    fn test_alt_up() {
        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(map_key_event(event), Action::MoveLineUp);
    }
}
