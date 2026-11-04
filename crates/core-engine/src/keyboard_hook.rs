use crate::events::{EngineEvent, KeyStroke};
use crate::injector::TextInjector;
use crate::matcher::SnippetMatcher;
use crate::privacy_guard::{PrivacyChecker, WindowsPrivacyGuard};
use crate::ring_buffer::SecureRingBuffer;
use crate::snippet::SnippetContent;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{error, info};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardState, GetKeyState, ToUnicodeEx, VIRTUAL_KEY, VK_BACK,
    VK_CAPITAL, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_SHIFT,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

/// Global dispatcher state for the low-level Windows keyboard hook.
struct HookContext {
    buffer: Arc<Mutex<SecureRingBuffer>>,
    privacy_guard: Arc<dyn PrivacyChecker>,
    event_sender: Option<Sender<EngineEvent>>,
    keyword_callback: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    matcher: Option<Arc<Mutex<SnippetMatcher>>>,
    injector: Option<Arc<TextInjector>>,
}

static GLOBAL_HOOK_CONTEXT: OnceLock<Mutex<Option<HookContext>>> = OnceLock::new();

fn get_hook_context() -> &'static Mutex<Option<HookContext>> {
    GLOBAL_HOOK_CONTEXT.get_or_init(|| Mutex::new(None))
}

/// Win32 low-level keyboard hook callback procedure.
#[cfg(target_os = "windows")]
unsafe extern "system" fn low_level_keyboard_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode < 0 {
        return CallNextHookEx(None, ncode, wparam, lparam);
    }

    let msg = wparam.0 as u32;
    if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
        let hook_struct = *(lparam.0 as *const KBDLLHOOKSTRUCT);

        // Check if injected (LLKHF_INJECTED = 0x10)
        let is_injected = (hook_struct.flags.0 & 0x10) != 0;

        if !is_injected {
            if let Ok(guard) = get_hook_context().lock() {
                if let Some(ctx) = guard.as_ref() {
                    handle_key_down(ctx, hook_struct.vkCode, hook_struct.scanCode);
                }
            }
        }
    }

    CallNextHookEx(None, ncode, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn handle_key_down(ctx: &HookContext, vk_code: u32, scan_code: u32) {
    // 1. Check Privacy Guard: if active control is a password field,
    // clear the buffer immediately and do not capture any keys!
    if ctx.privacy_guard.is_password_field_active() {
        if let Ok(mut buf) = ctx.buffer.lock() {
            if !buf.is_empty() {
                buf.clear();
                if let Some(ref sender) = ctx.event_sender {
                    let _ = sender.send(EngineEvent::BufferCleared);
                }
            }
        }
        if let Some(ref sender) = ctx.event_sender {
            let _ = sender.send(EngineEvent::PasswordFieldActive);
        }
        return;
    }

    // 2. Read modifier keys
    let shift = unsafe { (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 };
    let ctrl = unsafe { (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 };
    let alt = unsafe { (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 };

    // Ignore key combinations with Ctrl or Alt (shortcuts like Ctrl+C, Alt+F4)
    if ctrl || alt {
        return;
    }

    let vk = VIRTUAL_KEY(vk_code as u16);

    // 3. Handle Special Keys
    if vk == VK_BACK {
        if let Ok(mut buf) = ctx.buffer.lock() {
            buf.pop_back();
        }
        let stroke = KeyStroke::new(vk_code, scan_code, None, true, false, shift, ctrl, alt);
        if let Some(ref sender) = ctx.event_sender {
            let _ = sender.send(EngineEvent::KeyCaptured(stroke));
        }
        return;
    }

    if vk == VK_ESCAPE {
        if let Ok(mut buf) = ctx.buffer.lock() {
            buf.clear();
        }
        if let Some(ref sender) = ctx.event_sender {
            let _ = sender.send(EngineEvent::BufferCleared);
        }
        return;
    }

    // 4. Translate Virtual Key to Character using ToUnicodeEx
    let mut key_state = [0u8; 256];
    let _ = unsafe { GetKeyboardState(&mut key_state) };

    // In WH_KEYBOARD_LL, the hook thread does not have window focus,
    // so GetKeyboardState lacks the caller's modifier keys (Shift, CapsLock).
    // Synchronize modifier states from GetKeyState:
    if shift {
        key_state[VK_SHIFT.0 as usize] = 0x80;
    }
    if ctrl {
        key_state[VK_CONTROL.0 as usize] = 0x80;
    }
    if alt {
        key_state[VK_MENU.0 as usize] = 0x80;
    }
    let caps = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 };
    if caps {
        key_state[VK_CAPITAL.0 as usize] = 0x01;
    }

    let layout = unsafe { GetKeyboardLayout(0) };
    let mut unicode_buf = [0u16; 4];

    let result = unsafe {
        ToUnicodeEx(
            vk_code,
            scan_code,
            &key_state,
            &mut unicode_buf,
            0,
            Some(layout),
        )
    };

    if result > 0 {
        let chars = String::from_utf16_lossy(&unicode_buf[..result as usize]);
        for raw_ch in chars.chars() {
            // Normalize carriage return to newline
            let ch = if raw_ch == '\r' { '\n' } else { raw_ch };

            // Only buffer printable characters, spaces, and enter
            if !ch.is_control() || ch == '\n' || ch == '\t' {
                let current_content = if let Ok(mut buf) = ctx.buffer.lock() {
                    buf.push(ch);
                    buf.as_string()
                } else {
                    String::new()
                };

                let stroke =
                    KeyStroke::new(vk_code, scan_code, Some(ch), true, false, shift, ctrl, alt);
                if let Some(ref sender) = ctx.event_sender {
                    let _ = sender.send(EngineEvent::KeyCaptured(stroke));
                }

                // Check keyword callback
                if let Some(ref cb) = ctx.keyword_callback {
                    cb(&current_content);
                }

                // Check snippet match and auto-inject
                if let (Some(matcher_arc), Some(injector_arc)) = (&ctx.matcher, &ctx.injector) {
                    let matched = if let Ok(matcher) = matcher_arc.lock() {
                        matcher.find_match(&current_content)
                    } else {
                        None
                    };

                    if let Some(match_res) = matched {
                        match &match_res.snippet.content {
                            SnippetContent::PlainText(text) => {
                                injector_arc.inject(text, match_res.backspace_count);
                                if let Ok(mut buf) = ctx.buffer.lock() {
                                    buf.clear();
                                }
                                if let Some(ref sender) = ctx.event_sender {
                                    let _ = sender.send(EngineEvent::KeywordMatched(
                                        match_res.snippet.keyword,
                                    ));
                                }
                            }
                            SnippetContent::BitwardenRef { .. } => {
                                if let Some(ref sender) = ctx.event_sender {
                                    let _ = sender.send(EngineEvent::KeywordMatched(
                                        match_res.snippet.keyword,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Handle for controlling the background keyboard hook thread.
pub struct KeyboardHookHandle {
    thread_handle: Option<JoinHandle<()>>,
    #[cfg(target_os = "windows")]
    thread_id: u32,
    buffer: Arc<Mutex<SecureRingBuffer>>,
}

impl KeyboardHookHandle {
    /// Returns a reference to the shared secure ring buffer.
    pub fn buffer(&self) -> Arc<Mutex<SecureRingBuffer>> {
        Arc::clone(&self.buffer)
    }

    /// Stops the keyboard hook and waits for the thread to exit.
    pub fn stop(&mut self) {
        #[cfg(target_os = "windows")]
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        if let Ok(mut guard) = get_hook_context().lock() {
            *guard = None;
        }
    }
}

impl Drop for KeyboardHookHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Keyboard hook manager and builder.
pub struct KeyboardHook {
    buffer: Arc<Mutex<SecureRingBuffer>>,
    privacy_guard: Arc<dyn PrivacyChecker>,
    event_sender: Option<Sender<EngineEvent>>,
    keyword_callback: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    matcher: Option<Arc<Mutex<SnippetMatcher>>>,
    injector: Option<Arc<TextInjector>>,
}

impl KeyboardHook {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(SecureRingBuffer::new())),
            privacy_guard: Arc::new(WindowsPrivacyGuard::new()),
            event_sender: None,
            keyword_callback: None,
            matcher: None,
            injector: None,
        }
    }

    pub fn with_buffer(mut self, buffer: Arc<Mutex<SecureRingBuffer>>) -> Self {
        self.buffer = buffer;
        self
    }

    pub fn with_privacy_guard(mut self, guard: Arc<dyn PrivacyChecker>) -> Self {
        self.privacy_guard = guard;
        self
    }

    pub fn with_event_sender(mut self, sender: Sender<EngineEvent>) -> Self {
        self.event_sender = Some(sender);
        self
    }

    pub fn with_keyword_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.keyword_callback = Some(Arc::new(callback));
        self
    }

    pub fn with_matcher(mut self, matcher: Arc<Mutex<SnippetMatcher>>) -> Self {
        self.matcher = Some(matcher);
        self
    }

    pub fn with_injector(mut self, injector: Arc<TextInjector>) -> Self {
        self.injector = Some(injector);
        self
    }

    /// Starts the low-level keyboard hook on a dedicated background thread.
    pub fn start(self) -> Result<KeyboardHookHandle, anyhow::Error> {
        let (ready_tx, ready_rx) = channel::<Result<u32, String>>();

        let buffer_clone = Arc::clone(&self.buffer);

        // Store context in global static
        {
            let mut guard = get_hook_context()
                .lock()
                .map_err(|e| anyhow::anyhow!("Global hook context lock poisoned: {}", e))?;
            *guard = Some(HookContext {
                buffer: Arc::clone(&self.buffer),
                privacy_guard: self.privacy_guard,
                event_sender: self.event_sender,
                keyword_callback: self.keyword_callback,
                matcher: self.matcher,
                injector: self.injector,
            });
        }

        let thread_handle = thread::spawn(move || {
            #[cfg(target_os = "windows")]
            {
                let thread_id = unsafe { GetCurrentThreadId() };

                let hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(low_level_keyboard_proc),
                        None,
                        0,
                    )
                };

                let hhook = match hook {
                    Ok(h) => {
                        let _ = ready_tx.send(Ok(thread_id));
                        h
                    }
                    Err(e) => {
                        let err_msg = format!("SetWindowsHookExW failed: {:?}", e);
                        error!("{}", err_msg);
                        let _ = ready_tx.send(Err(err_msg));
                        return;
                    }
                };

                info!("Windows Low-Level Keyboard Hook started on thread {}", thread_id);

                // Run standard Win32 message pump
                let mut msg = MSG::default();
                unsafe {
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    let _ = UnhookWindowsHookEx(hhook);
                }

                info!("Windows Low-Level Keyboard Hook stopped cleanly");
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = ready_tx.send(Ok(0));
            }
        });

        let thread_id = ready_rx
            .recv_timeout(Duration::from_secs(3))
            .map_err(|e| anyhow::anyhow!("Timed out waiting for hook thread initialization: {}", e))?
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(KeyboardHookHandle {
            thread_handle: Some(thread_handle),
            #[cfg(target_os = "windows")]
            thread_id,
            buffer: buffer_clone,
        })
    }
}

impl Default for KeyboardHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_guard::MockPrivacyGuard;
    use std::time::Duration;

    #[test]
    fn test_keyboard_hook_with_matcher() {
        let mock_guard = Arc::new(MockPrivacyGuard::new(false));
        let (tx, _rx) = channel();

        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(crate::snippet::Snippet::plain_text(
            "test_1",
            "Hello",
            ":hi",
            "Hello, Challa!",
        ));
        let matcher_arc = Arc::new(Mutex::new(matcher));
        let injector_arc = Arc::new(TextInjector::new());

        let hook = KeyboardHook::new()
            .with_privacy_guard(mock_guard)
            .with_matcher(matcher_arc)
            .with_injector(injector_arc)
            .with_event_sender(tx);

        let mut handle = hook.start().expect("Hook should start with matcher");
        assert!(handle.thread_handle.is_some());

        thread::sleep(Duration::from_millis(50));
        handle.stop();
    }
}
