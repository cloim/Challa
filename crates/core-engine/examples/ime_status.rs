//! Read-only input metadata. Does not read text, send keys, or move focus.
use challa_core::{PrivacyChecker, WindowsPrivacyGuard};
use windows::Win32::UI::{
    Input::{Ime::*, KeyboardAndMouse::*},
    WindowsAndMessaging::*,
};
fn main() {
    unsafe {
        let foreground = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(foreground, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let focus_known = GetGUIThreadInfo(thread, &mut gui).is_ok();
        let imc = ImmGetContext(gui.hwndFocus);
        let mut conversion = IME_CONVERSION_MODE(0);
        let legacy_known =
            !imc.0.is_null() && ImmGetConversionStatus(imc, Some(&mut conversion), None).as_bool();
        if !imc.0.is_null() {
            let _ = ImmReleaseContext(gui.hwndFocus, imc);
        }
        let ime = ImmGetDefaultIMEWnd(gui.hwndFocus);
        let mut mode = 0usize;
        let window_known = !ime.is_invalid()
            && SendMessageTimeoutW(
                ime,
                WM_IME_CONTROL,
                windows::Win32::Foundation::WPARAM(1),
                windows::Win32::Foundation::LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                40,
                Some(&mut mode),
            )
            .0 != 0;
        println!("focus_known={focus_known}, korean_layout={}, legacy_context={}, legacy_mode_known={legacy_known}, legacy_native={}, ime_window_known={window_known}, ime_window_native={}", GetKeyboardLayout(thread).0 as usize & 0x3ff == 0x12, !imc.0.is_null(), conversion.0 & IME_CMODE_NATIVE.0 != 0, mode & IME_CMODE_NATIVE.0 as usize != 0);
    }
    let guard = WindowsPrivacyGuard::new();
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("privacy_input_blocked={}", guard.is_input_blocked());
}
