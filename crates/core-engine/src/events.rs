use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStroke {
    pub vk_code: u32,
    pub scan_code: u32,
    pub character: Option<char>,
    pub is_key_down: bool,
    pub is_injected: bool,
    pub is_backspace: bool,
    pub is_enter: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyStroke {
    pub fn new(
        vk_code: u32,
        scan_code: u32,
        character: Option<char>,
        is_key_down: bool,
        is_injected: bool,
        shift: bool,
        ctrl: bool,
        alt: bool,
    ) -> Self {
        // VK_BACK = 0x08, VK_RETURN = 0x0D
        let is_backspace = vk_code == 0x08;
        let is_enter = vk_code == 0x0D;

        Self {
            vk_code,
            scan_code,
            character,
            is_key_down,
            is_injected,
            is_backspace,
            is_enter,
            shift,
            ctrl,
            alt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineEvent {
    KeyCaptured(KeyStroke),
    KeywordMatched(String),
    PasswordFieldActive,
    BufferCleared,
    InteractiveExpansion {
        snippet_id: String,
        target: (usize, usize),
        expected: String,
        revision: u64,
    },
    StatusChanged,
}
