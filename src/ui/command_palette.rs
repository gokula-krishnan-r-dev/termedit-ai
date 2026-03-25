/// Command palette (placeholder for MVP).
///
/// Full fuzzy-search implementation will come in a later milestone.

pub struct CommandPalette {
    pub visible: bool,
    pub input: String,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
        }
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
