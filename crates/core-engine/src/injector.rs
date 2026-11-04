use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, error, trace, warn};

#[cfg(target_os = "windows")]
use windows::core::w;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
#[cfg(target_os = "windows")]
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL,
};

// Standard CF_UNICODETEXT is 13
const CF_UNICODETEXT_ID: u32 = 13;

/// Threshold length under which text is injected directly via Unicode keystrokes.
pub const DIRECT_UNICODE_THRESHOLD: usize = 64;

/// Text injection engine supporting both secure direct typing and safe clipboard fallback.
#[derive(Debug, Default, Clone)]
pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    /// Erases `count` characters using synthesized Backspace events.
    pub fn erase_characters(&self, count: usize) {
        if count == 0 {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            let mut inputs = Vec::with_capacity(count * 2);

            for _ in 0..count {
                // Key down
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_BACK,
                            wScan: 0,
                            dwFlags: KEYBD_EVENT_FLAGS(0),
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });

                // Key up
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_BACK,
                            wScan: 0,
                            dwFlags: KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }

            unsafe {
                let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
                trace!("SendInput erased {} backspaces (sent: {})", count, sent);
            }
        }
    }

    /// Directly types Unicode characters via `SendInput(KEYEVENTF_UNICODE)`.
    /// Completely bypasses clipboard; 100% private and ideal for passwords and short snippets.
    pub fn inject_unicode_text(&self, text: &str) {
        if text.is_empty() {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            let utf16_units: Vec<u16> = text.encode_utf16().collect();
            let mut inputs = Vec::with_capacity(utf16_units.len() * 2);

            for &unit in &utf16_units {
                // Key down with KEYEVENTF_UNICODE
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0),
                            wScan: unit,
                            dwFlags: KEYEVENTF_UNICODE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });

                // Key up with KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0),
                            wScan: unit,
                            dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }

            unsafe {
                let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
                debug!(
                    "SendInput directly typed {} unicode code units (sent: {})",
                    utf16_units.len(),
                    sent
                );
            }
        }
    }

    /// Injects multiline or large text through the Windows clipboard with privacy flags,
    /// triggering Ctrl+V and immediately restoring the original clipboard content.
    pub fn inject_safe_clipboard(&self, text: &str) {
        #[cfg(target_os = "windows")]
        {
            let original_content = self.read_clipboard_text();

            if self.set_clipboard_text_with_privacy_guard(text) {
                // Simulate Ctrl+V (Paste)
                self.simulate_paste();

                // Wait for the recipient window to consume the paste command
                sleep(Duration::from_millis(35));

                // Restore previous clipboard content
                if let Some(prev) = original_content {
                    let _ = self.set_clipboard_text_with_privacy_guard(&prev);
                } else {
                    self.clear_clipboard();
                }
            } else {
                warn!("Clipboard write failed, falling back to direct Unicode typing");
                self.inject_unicode_text(text);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = text;
        }
    }

    /// High-level injection entry point: erases keyword and injects snippet content.
    pub fn inject(&self, text: &str, backspace_count: usize) {
        self.erase_characters(backspace_count);

        let is_short_single_line =
            text.len() <= DIRECT_UNICODE_THRESHOLD && !text.contains('\n') && !text.contains('\r');

        if is_short_single_line {
            self.inject_unicode_text(text);
        } else {
            self.inject_safe_clipboard(text);
        }
    }

    #[cfg(target_os = "windows")]
    fn simulate_paste(&self) {
        // VK_V is 0x56
        let vk_v = VIRTUAL_KEY(0x56);

        let inputs = [
            // Ctrl Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // V Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk_v,
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // V Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk_v,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Ctrl Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        unsafe {
            let _ = SendInput(&inputs, size_of::<INPUT>() as i32);
        }
    }

    #[cfg(target_os = "windows")]
    fn open_clipboard_with_retry() -> bool {
        for _ in 0..5 {
            if unsafe { OpenClipboard(None).is_ok() } {
                return true;
            }
            sleep(Duration::from_millis(10));
        }
        false
    }

    #[cfg(target_os = "windows")]
    fn clear_clipboard(&self) {
        if Self::open_clipboard_with_retry() {
            unsafe {
                let _ = EmptyClipboard();
                let _ = CloseClipboard();
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn read_clipboard_text(&self) -> Option<String> {
        if !Self::open_clipboard_with_retry() {
            return None;
        }

        let result = unsafe {
            let handle_res = GetClipboardData(CF_UNICODETEXT_ID);
            if let Ok(handle) = handle_res {
                let ptr = GlobalLock(HGLOBAL(handle.0));
                if !ptr.is_null() {
                    let wide_slice = std::slice::from_raw_parts(
                        ptr as *const u16,
                        usize::MAX / 2, // bounded by null-terminator below
                    );
                    let mut len = 0;
                    while len < wide_slice.len() && wide_slice[len] != 0 {
                        len += 1;
                    }
                    let s = String::from_utf16_lossy(&wide_slice[..len]);
                    let _ = GlobalUnlock(HGLOBAL(handle.0));
                    Some(s)
                } else {
                    None
                }
            } else {
                None
            }
        };

        unsafe {
            let _ = CloseClipboard();
        }

        result
    }

    #[cfg(target_os = "windows")]
    fn set_clipboard_text_with_privacy_guard(&self, text: &str) -> bool {
        if !Self::open_clipboard_with_retry() {
            return false;
        }

        unsafe {
            let _ = EmptyClipboard();

            // 1. Write Unicode Text
            let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes_len = utf16.len() * size_of::<u16>();

            let h_mem = match GlobalAlloc(GMEM_MOVEABLE, bytes_len) {
                Ok(h) => h,
                Err(e) => {
                    error!("GlobalAlloc failed: {:?}", e);
                    let _ = CloseClipboard();
                    return false;
                }
            };

            let ptr = GlobalLock(h_mem);
            if ptr.is_null() {
                let _ = CloseClipboard();
                return false;
            }

            std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const u8, ptr as *mut u8, bytes_len);
            let _ = GlobalUnlock(h_mem);

            if let Err(e) = SetClipboardData(CF_UNICODETEXT_ID, Some(HANDLE(h_mem.0))) {
                warn!("SetClipboardData(CF_UNICODETEXT) failed: {:?}", e);
            }

            // 2. Set Privacy Flags so Windows Clipboard History & monitors ignore it:
            // "CanIncludeInClipboardHistory" = DWORD 0
            // "ExcludeClipboardContentFromMonitorProcessing" = DWORD 0
            let fmt_history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
            if fmt_history != 0 {
                if let Ok(h_priv) = GlobalAlloc(GMEM_MOVEABLE, 4) {
                    let p = GlobalLock(h_priv);
                    if !p.is_null() {
                        *(p as *mut u32) = 0;
                        let _ = GlobalUnlock(h_priv);
                        let _ = SetClipboardData(fmt_history, Some(HANDLE(h_priv.0)));
                    }
                }
            }

            let fmt_monitor =
                RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
            if fmt_monitor != 0 {
                if let Ok(h_priv) = GlobalAlloc(GMEM_MOVEABLE, 4) {
                    let p = GlobalLock(h_priv);
                    if !p.is_null() {
                        *(p as *mut u32) = 0;
                        let _ = GlobalUnlock(h_priv);
                        let _ = SetClipboardData(fmt_monitor, Some(HANDLE(h_priv.0)));
                    }
                }
            }

            let _ = CloseClipboard();
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injector_instantiation() {
        let injector = TextInjector::new();
        // Erase 0 characters should safely do nothing
        injector.erase_characters(0);
    }
}
