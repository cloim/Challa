use challa_bitwarden::BitwardenField;
use serde::{Deserialize, Serialize};

/// Beeftext-compatible triggering condition for a snippet keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    /// Expands immediately as soon as the keyword matches.
    Immediate,
    /// Expands when keyword is followed by Space (' ').
    Space,
    /// Expands when keyword is followed by Enter ('\r' or '\n').
    Enter,
    /// Expands when keyword is followed by Tab ('\t').
    Tab,
    /// Expands when keyword is followed by any standard delimiter
    /// (Space, Enter, Tab, comma, period, semicolon, etc.).
    Delimiter,
}

impl Default for TriggerMode {
    fn default() -> Self {
        Self::Immediate
    }
}

/// The content payload of a snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnippetContent {
    /// Plain static or macro text.
    PlainText(String),
    /// Reference to a Bitwarden vault item field.
    BitwardenRef {
        item_id: String,
        field: BitwardenField,
    },
}

/// A snippet record in Challa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub keyword: String,
    pub content: SnippetContent,
    pub trigger_mode: TriggerMode,
    pub enabled: bool,
}

impl Snippet {
    /// Creates a plain text snippet with immediate trigger mode.
    pub fn plain_text(id: impl Into<String>, name: impl Into<String>, keyword: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            keyword: keyword.into(),
            content: SnippetContent::PlainText(text.into()),
            trigger_mode: TriggerMode::Immediate,
            enabled: true,
        }
    }

    /// Creates a plain text snippet with a specific trigger mode.
    pub fn plain_text_with_trigger(
        id: impl Into<String>,
        name: impl Into<String>,
        keyword: impl Into<String>,
        text: impl Into<String>,
        trigger_mode: TriggerMode,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            keyword: keyword.into(),
            content: SnippetContent::PlainText(text.into()),
            trigger_mode,
            enabled: true,
        }
    }

    /// Creates a Bitwarden vault snippet.
    pub fn bitwarden_ref(
        id: impl Into<String>,
        name: impl Into<String>,
        keyword: impl Into<String>,
        item_id: impl Into<String>,
        field: BitwardenField,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            keyword: keyword.into(),
            content: SnippetContent::BitwardenRef {
                item_id: item_id.into(),
                field,
            },
            trigger_mode: TriggerMode::Immediate,
            enabled: true,
        }
    }
}
