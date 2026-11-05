use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Hook callers may only read a bounded cache, never call a UI provider.
pub trait PrivacyChecker: Send + Sync {
    fn is_password_field_active(&self) -> bool;
    fn is_input_blocked(&self) -> bool {
        self.is_password_field_active()
    }
    fn invalidate(&self) {}
}

#[derive(Debug, Default, Clone)]
pub struct MockPrivacyGuard {
    is_password: Arc<AtomicBool>,
}
impl MockPrivacyGuard {
    pub fn new(initial: bool) -> Self {
        Self {
            is_password: Arc::new(AtomicBool::new(initial)),
        }
    }
    pub fn set_password_active(&self, active: bool) {
        self.is_password.store(active, Ordering::Release);
    }
}
impl PrivacyChecker for MockPrivacyGuard {
    fn is_password_field_active(&self) -> bool {
        self.is_password.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
struct PrivacyState {
    blocked: AtomicBool,
    password: AtomicBool,
    foreground: AtomicUsize,
    focus: AtomicUsize,
    generation: AtomicU64,
    checked_generation: AtomicU64,
    checked_at: AtomicU64,
    clock: Instant,
}

impl PrivacyState {
    fn input_blocked(&self, target: (usize, usize)) -> bool {
        let checked = self.checked_at.load(Ordering::Acquire);
        checked == 0
            || (self.clock.elapsed().as_millis() as u64 + 1).saturating_sub(checked) > 200
            || self.checked_generation.load(Ordering::Acquire)
                != self.generation.load(Ordering::Acquire)
            || target.0 != self.foreground.load(Ordering::Acquire)
            || target.1 != self.focus.load(Ordering::Acquire)
            || self.blocked.load(Ordering::Acquire)
    }
}

/// One COM worker queries UIA. Changed, unknown or stale focus fails closed.
#[derive(Debug, Clone)]
pub struct WindowsPrivacyGuard {
    state: Arc<PrivacyState>,
}

#[cfg(target_os = "windows")]
pub(crate) fn input_target() -> (usize, usize) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let foreground = GetForegroundWindow();
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if foreground.is_invalid()
            || GetGUIThreadInfo(GetWindowThreadProcessId(foreground, None), &mut gui).is_err()
        {
            return (0, 0);
        }
        (foreground.0 as usize, gui.hwndFocus.0 as usize)
    }
}

impl WindowsPrivacyGuard {
    pub fn new() -> Self {
        let state = Arc::new(PrivacyState {
            blocked: AtomicBool::new(true),
            password: AtomicBool::new(false),
            foreground: AtomicUsize::new(0),
            focus: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            checked_at: AtomicU64::new(0),
            clock: Instant::now(),
            checked_generation: AtomicU64::new(u64::MAX),
        });
        #[cfg(target_os = "windows")]
        {
            let weak = Arc::downgrade(&state);
            std::thread::spawn(move || {
                use windows::Win32::System::Com::*;
                use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
                unsafe {
                    if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                        return;
                    }
                    let automation = CoCreateInstance::<_, IUIAutomation>(
                        &CUIAutomation,
                        None,
                        CLSCTX_INPROC_SERVER,
                    );
                    if let Ok(automation) = automation {
                        while let Some(state) = weak.upgrade() {
                            let generation = state.generation.load(Ordering::Acquire);
                            let target = input_target();
                            let password = automation
                                .GetFocusedElement()
                                .and_then(|element| element.CurrentIsPassword())
                                .map(|value| value.as_bool());
                            if target != (0, 0)
                                && target == input_target()
                                && generation == state.generation.load(Ordering::Acquire)
                            {
                                state.foreground.store(target.0, Ordering::Release);
                                state.focus.store(target.1, Ordering::Release);
                                state.password.store(
                                    password.as_ref().copied().unwrap_or(false),
                                    Ordering::Release,
                                );
                                state.blocked.store(password.is_err(), Ordering::Release);
                                state
                                    .checked_generation
                                    .store(generation, Ordering::Release);
                                state.checked_at.store(
                                    state.clock.elapsed().as_millis() as u64 + 1,
                                    Ordering::Release,
                                );
                            }
                            drop(state);
                            std::thread::sleep(Duration::from_millis(25));
                        }
                    }
                    CoUninitialize();
                }
            });
        }
        Self { state }
    }
    pub fn is_cached_password_active(&self) -> bool {
        self.is_password_field_active()
    }
}
impl Default for WindowsPrivacyGuard {
    fn default() -> Self {
        Self::new()
    }
}
impl PrivacyChecker for WindowsPrivacyGuard {
    fn invalidate(&self) {
        self.state.generation.fetch_add(1, Ordering::AcqRel);
        self.state.blocked.store(true, Ordering::Release);
        self.state.checked_at.store(0, Ordering::Release);
    }
    fn is_password_field_active(&self) -> bool {
        self.is_input_blocked() || self.state.password.load(Ordering::Acquire)
    }
    fn is_input_blocked(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.state.input_blocked(input_target())
        }
        #[cfg(not(target_os = "windows"))]
        {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmed_password_focus_is_allowed_but_unknown_focus_is_blocked() {
        let state = PrivacyState {
            blocked: AtomicBool::new(false),
            password: AtomicBool::new(true),
            foreground: AtomicUsize::new(1),
            focus: AtomicUsize::new(2),
            generation: AtomicU64::new(3),
            checked_generation: AtomicU64::new(3),
            checked_at: AtomicU64::new(1),
            clock: Instant::now(),
        };
        assert!(!state.input_blocked((1, 2)));
        assert!(state.password.load(Ordering::Acquire));
        assert!(state.input_blocked((1, 4)));
        state.generation.fetch_add(1, Ordering::AcqRel);
        assert!(state.input_blocked((1, 2)));
    }
    #[test]
    fn password_cache_changes_without_a_provider_call() {
        let guard = MockPrivacyGuard::new(false);
        assert!(!guard.is_password_field_active());
        guard.set_password_active(true);
        assert!(guard.is_password_field_active());
    }
    #[test]
    fn unknown_focus_is_blocked_immediately() {
        let guard = WindowsPrivacyGuard::new();
        guard.invalidate();
        assert!(guard.is_password_field_active());
    }
}
