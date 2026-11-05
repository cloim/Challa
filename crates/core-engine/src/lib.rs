pub mod events;
pub mod injector;
pub mod interactive;
pub mod keyboard_hook;
pub mod korean;
pub mod matcher;
pub mod privacy_guard;
pub mod profiles;
pub mod ring_buffer;
pub mod snippet;
pub mod template;

pub use events::{EngineEvent, KeyStroke};
pub use injector::TextInjector;
pub use keyboard_hook::{KeyboardHook, KeyboardHookHandle};
pub use matcher::{MatchResult, SnippetMatcher};
pub use privacy_guard::{MockPrivacyGuard, PrivacyChecker, WindowsPrivacyGuard};
pub use ring_buffer::{SecureRingBuffer, DEFAULT_BUFFER_CAPACITY, DEFAULT_IDLE_TIMEOUT};
pub use snippet::{Snippet, SnippetContent, TriggerMode};
