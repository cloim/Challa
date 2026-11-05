//! Foreground targets are captured in Rust; the webview receives no native handles.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::*, System::Threading::*, UI::Input::KeyboardAndMouse::*, UI::WindowsAndMessaging::*,
};

pub fn current_target() -> (usize, usize) {
    crate::privacy_guard::input_target()
}
pub fn process_id(target: (usize, usize)) -> u32 {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(HWND(target.0 as *mut _), Some(&mut pid));
    }
    pid
}
pub fn application(target: (usize, usize)) -> String {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id(target))
        else {
            return String::new();
        };
        let mut path = [0u16; 1024];
        let mut length = path.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        if result.is_err() {
            return String::new();
        }
        String::from_utf16_lossy(&path[..length as usize])
            .rsplit('\\')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
}
pub fn restore(target: (usize, usize), pid: u32) -> anyhow::Result<()> {
    if target.0 == 0
        || target.1 == 0
        || process_id(target) != pid
        || process_id((target.1, 0)) != pid
    {
        anyhow::bail!("원래 입력창을 다시 선택해 주세요");
    }
    unsafe {
        if !IsWindow(Some(HWND(target.0 as *mut _))).as_bool()
            || !IsWindow(Some(HWND(target.1 as *mut _))).as_bool()
        {
            anyhow::bail!("원래 입력창이 닫혔어요");
        }
        if !SetForegroundWindow(HWND(target.0 as *mut _)).as_bool() {
            anyhow::bail!("원래 입력창으로 돌아갈 수 없어요");
        }
    }
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        let modifiers_up = unsafe {
            [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
                .iter()
                .all(|k| GetAsyncKeyState(k.0 as i32) >= 0)
        };
        if current_target() == target && modifiers_up {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    anyhow::bail!("입력 위치가 바뀌었어요. 원하는 위치에서 다시 실행해 주세요")
}
pub fn insert(
    text: &str,
    expected: &str,
    target: (usize, usize),
    privacy: &dyn crate::PrivacyChecker,
    cancelled: &AtomicBool,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_millis(220);
    privacy.invalidate();
    while privacy.is_input_blocked()
        && Instant::now() < deadline
        && current_target() == target
        && !cancelled.load(Ordering::Acquire)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    if current_target() != target || privacy.is_input_blocked() || cancelled.load(Ordering::Acquire)
    {
        anyhow::bail!("입력창 보호 상태를 확인해 주세요");
    }
    let mut session = crate::injector::InjectionSession::new()?;
    let result = session.substitute(
        text,
        expected.chars().count(),
        expected,
        &[],
        target,
        0,
        cancelled,
        privacy.is_password_field_active(),
    );
    let deadline = Instant::now() + Duration::from_millis(1500);
    while !session.clipboard_wait().is_zero() && Instant::now() < deadline {
        crate::injector::pump_clipboard_messages();
        std::thread::sleep(Duration::from_millis(5));
    }
    session.restore_clipboard();
    result
}
