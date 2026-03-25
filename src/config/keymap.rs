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
    GoToSymbol,
    /// Jump between `()`, `[]`, `{}` (VS Code: Ctrl/Cmd+Shift+\).
    GoToMatchingBracket,

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
    DuplicateLine,

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
    /// Search across all open tabs (Ctrl/Cmd+Shift+F).
    FindInOpenTabs,
    FindNext,
    FindPrev,
    EscapeSearch,

    // === View ===
    ToggleFileTree,
    ToggleAiPanel,
    /// Insert the last AI assistant reply at the cursor (panel focused).
    AiInsertLastReply,
    /// Open the AI assistant with a brainstorm-ideas prompt prefilled.
    AiBrainstorm,
    CommandPalette,

    // === Tabs ===
    NextTab,
    PrevTab,
    /// Switch to tab index 0–8 (Alt+1 … Alt+9).
    GoToTab(usize),

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
        KeyCode::PageUp if ctrl => Action::PrevTab,
        KeyCode::PageDown if ctrl => Action::NextTab,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::F(3) if shift => Action::FindPrev,
        KeyCode::F(3) => Action::FindNext,

        // Alt+1 … Alt+9 → tabs 0…8
        KeyCode::Char('1') if alt => Action::GoToTab(0),
        KeyCode::Char('2') if alt => Action::GoToTab(1),
        KeyCode::Char('3') if alt => Action::GoToTab(2),
        KeyCode::Char('4') if alt => Action::GoToTab(3),
        KeyCode::Char('5') if alt => Action::GoToTab(4),
        KeyCode::Char('6') if alt => Action::GoToTab(5),
        KeyCode::Char('7') if alt => Action::GoToTab(6),
        KeyCode::Char('8') if alt => Action::GoToTab(7),
        KeyCode::Char('9') if alt => Action::GoToTab(8),

        // === Ctrl/Cmd shortcuts ===
        KeyCode::Char('q') if ctrl && shift => Action::ForceQuit,
        KeyCode::Char('q') if ctrl => Action::Quit,
        KeyCode::Char('s') if (ctrl || super_) && shift => Action::SaveAs,
        KeyCode::Char('s') if ctrl || super_ => Action::Save,
        KeyCode::Char('z') if (ctrl || super_) && shift => Action::Redo,
        KeyCode::Char('z') if ctrl || super_ => Action::Undo,
        KeyCode::Char('y') if ctrl || super_ => Action::Redo,
        KeyCode::Char('c') if ctrl || super_ => Action::Copy,
        KeyCode::Char('x') if ctrl || super_ => Action::Cut,
        KeyCode::Char('v') if ctrl || super_ => Action::Paste,
        KeyCode::Char('a') if ctrl || super_ => Action::SelectAll,
        KeyCode::Char('l') if ctrl || super_ => Action::SelectLine,
        KeyCode::Char('d') if (ctrl || super_) && shift => Action::DuplicateLine,
        KeyCode::Char('d') if ctrl || super_ => Action::DeleteLine,
        KeyCode::Char('f') if (ctrl || super_) && shift => Action::FindInOpenTabs,
        KeyCode::Char('f') if ctrl || super_ => Action::Find,
        KeyCode::Char('h') if ctrl || super_ => Action::FindReplace,
        KeyCode::Char('g') if ctrl || super_ => Action::GoToLine,
        KeyCode::Char('o') if (ctrl || super_) && shift => Action::GoToSymbol,
        KeyCode::Char('\\') if (ctrl || super_) && shift => Action::GoToMatchingBracket,
        KeyCode::Char('n') if ctrl || super_ => Action::NewFile,
        KeyCode::Char('o') if ctrl || super_ => Action::OpenFile,
        KeyCode::Char('w') if ctrl || super_ => Action::CloseBuffer,
        KeyCode::Char('b') if ctrl || super_ => Action::ToggleFileTree,
        KeyCode::Char('k') if ctrl || super_ => Action::ToggleAiPanel,
        KeyCode::Char('i') if (ctrl || super_) && shift => Action::AiInsertLastReply,
        KeyCode::Char('u') if (ctrl || super_) && shift => Action::AiBrainstorm,
        KeyCode::Char('p') if ctrl || super_ => Action::CommandPalette,
        KeyCode::Char('/') if ctrl || super_ => Action::ToggleComment,
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

    #[test]
    fn test_f3_find_next_prev() {
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            Action::FindNext
        );
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT)),
            Action::FindPrev
        );
    }

    #[test]
    fn test_ctrl_page_tab_cycle() {
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL)),
            Action::PrevTab
        );
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL)),
            Action::NextTab
        );
    }

    #[test]
    fn test_alt_digit_tab() {
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::ALT)),
            Action::GoToTab(1)
        );
    }

    #[test]
    fn test_ctrl_shift_q_force_quit() {
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
            Action::ForceQuit
        );
    }

    #[test]
    fn test_cmd_z_undo() {
        assert_eq!(
            map_key_event(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::SUPER)),
            Action::Undo
        );
    }

    #[test]
    fn test_ctrl_shift_backslash_matching_bracket() {
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('\\'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Action::GoToMatchingBracket
        );
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('\\'),
                KeyModifiers::SUPER | KeyModifiers::SHIFT
            )),
            Action::GoToMatchingBracket
        );
    }

    #[test]
    fn test_ctrl_shift_o_goto_symbol() {
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('o'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Action::GoToSymbol
        );
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('o'),
                KeyModifiers::SUPER | KeyModifiers::SHIFT
            )),
            Action::GoToSymbol
        );
    }

    #[test]
    fn test_ctrl_shift_u_ai_brainstorm() {
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Action::AiBrainstorm
        );
    }

    #[test]
    fn test_ctrl_shift_f_find_in_open_tabs() {
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Action::FindInOpenTabs
        );
        assert_eq!(
            map_key_event(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::SUPER | KeyModifiers::SHIFT
            )),
            Action::FindInOpenTabs
        );
    }
}
