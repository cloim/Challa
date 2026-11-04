use std::mem::size_of;
use std::thread::{self, sleep};
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
    GetKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_LCONTROL, VK_LMENU,
    VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_SHIFT,
};

// Standard CF_UNICODETEXT is 13
const CF_UNICODETEXT_ID: u32 = 13;

/// Text injection engine following Beeftext's proven architecture:
/// 1. Backs up and releases modifier keys (Shift, Ctrl, Alt) to prevent key combination corruption.
/// 2. Synthesizes backspaces with micro-pauses for target application message queues.
/// 3. Pastes snippet via clipboard with Windows privacy flags.
/// 4. Restores modifier keys.
/// 5. Schedules a delayed clipboard restore (1000ms) so target app reliably finishes reading.
#[derive(Debug, Default, Clone)]
pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    /// Backs up currently pressed modifier keys and synthesizes key-up events for them.
    #[cfg(target_os = "windows")]
    pub fn backup_and_release_modifiers(&self) -> Vec<VIRTUAL_KEY> {
        let modifier_keys = [
            VK_LCONTROL,
            VK_RCONTROL,
            VK_CONTROL,
            VK_LMENU,
            VK_RMENU,
            VK_MENU,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_SHIFT,
        ];

        let mut pressed = Vec::new();
        for &vk in &modifier_keys {
            let state = unsafe { GetKeyState(vk.0 as i32) as u16 };
            if (state & 0x8000) != 0 {
                pressed.push(vk);
                self.synthesize_key_event(vk, true); // key up
            }
        }
        pressed
    }

    /// Restores previously pressed modifier keys by synthesizing key-down events.
    #[cfg(target_os = "windows")]
    pub fn restore_modifiers(&self, keys: &[VIRTUAL_KEY]) {
        for &vk in keys {
            self.synthesize_key_event(vk, false); // key down
        }
    }

    /// Synthesizes a single key down or key up event.
    #[cfg(target_os = "windows")]
    fn synthesize_key_event(&self, vk: VIRTUAL_KEY, is_up: bool) {
        let dw_flags = if is_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        let input = [INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: dw_flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }];
        unsafe {
            let _ = SendInput(&input, size_of::<INPUT>() as i32);
        }
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

    /// Simulates Ctrl+V (Paste).
    #[cfg(target_os = "windows")]
    pub fn simulate_paste(&self) {
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

    /// Performs the complete text substitution sequence following Beeftext's robust method:
    /// 1. Short pause (15ms) for the triggering key to register in the target window.
    /// 2. Release any physically held modifier keys (e.g. Shift from typing ':').
    /// 3. Synthesize Backspaces to erase the keyword.
    /// 4. Short pause (15ms) for target window to process Backspaces.
    /// 5. Back up existing clipboard content.
    /// 6. Set new snippet text in clipboard with privacy flags.
    /// 7. Send Ctrl+V.
    /// 8. Restore modifier keys.
    /// 9. Asynchronously restore original clipboard content after 1000ms.
    pub fn perform_substitution(&self, text: &str, backspace_count: usize) {
        #[cfg(target_os = "windows")]
        {
            // Step 1: Wait 15ms for target app to finish rendering the triggering keystroke
            sleep(Duration::from_millis(15));

            // Step 2: Release modifiers (e.g. Shift key from typing ':')
            let pressed_mods = self.backup_and_release_modifiers();

            // Step 3: Erase the keyword
            if backspace_count > 0 {
                self.erase_characters(backspace_count);
            }

            // Step 4: Wait 15ms for target editor to process backspaces
            sleep(Duration::from_millis(15));

            // Step 5: Read original clipboard
            let original_clipboard = self.read_clipboard_text();

            // Step 6: Ensure CRLF line endings for Windows apps (Notepad, etc.)
            let formatted_text = if text.contains('\n') && !text.contains("\r\n") {
                text.replace('\n', "\r\n")
            } else {
                text.to_string()
            };

            // Step 7: Set clipboard with privacy flags and Paste
            if self.set_clipboard_text_with_privacy_guard(&formatted_text) {
                self.simulate_paste();

                // Step 8: Restore modifier keys
                self.restore_modifiers(&pressed_mods);

                // Step 9: Schedule clipboard restoration after 1000ms (Beeftext standard)
                // in an isolated background thread so it never blocks the editor!
                let prev_content = original_clipboard;
                thread::spawn(move || {
                    sleep(Duration::from_millis(1000));
                    let injector = TextInjector::new();
                    if let Some(prev) = prev_content {
                        let _ = injector.set_clipboard_text_with_privacy_guard(&prev);
                    } else {
                        injector.clear_clipboard();
                    }
                    debug!("Original clipboard content restored");
                });
            } else {
                warn!("Clipboard insertion failed, falling back to direct typing");
                self.inject_unicode_text(text);
                self.restore_modifiers(&pressed_mods);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (text, backspace_count);
        }
    }

    /// Directly types Unicode characters via `SendInput(KEYEVENTF_UNICODE)`.
    pub fn inject_unicode_text(&self, text: &str) {
        if text.is_empty() {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            let utf16_units: Vec<u16> = text.encode_utf16().collect();
            let mut inputs = Vec::with_capacity(utf16_units.len() * 2);

            for &unit in &utf16_units {
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
                let _ = SendInput(&inputs, size_of::<INPUT>() as i32);
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn open_clipboard_with_retry() -> bool {
        for _ in 0..10 {
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
        injector.erase_characters(0);
    }
}
