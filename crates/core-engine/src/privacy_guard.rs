use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, trace};

/// Trait defining a privacy detector that checks if sensitive input
/// (e.g. password field) is currently active.
pub trait PrivacyChecker: Send + Sync {
    fn is_password_field_active(&self) -> bool;
}

/// Mock privacy checker for tests and non-Windows environments.
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
        self.is_password.store(active, Ordering::SeqCst);
    }
}

impl PrivacyChecker for MockPrivacyGuard {
    fn is_password_field_active(&self) -> bool {
        self.is_password.load(Ordering::SeqCst)
    }
}

/// Windows UI Automation based privacy guard.
/// Queries whether the currently focused control has `IsPassword == TRUE`.
#[derive(Debug, Clone)]
pub struct WindowsPrivacyGuard {
    cached_state: Arc<AtomicBool>,
}

impl WindowsPrivacyGuard {
    pub fn new() -> Self {
        Self {
            cached_state: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns the last cached state of password field detection.
    pub fn is_cached_password_active(&self) -> bool {
        self.cached_state.load(Ordering::SeqCst)
    }

    /// Checks UI Automation directly for the focused element and updates cache.
    #[cfg(target_os = "windows")]
    fn check_focused_element_is_password(&self) -> bool {
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

        unsafe {
            // Ensure COM is initialized for the current thread
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let automation_res =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER);

            let automation = match automation_res {
                Ok(auto) => auto,
                Err(e) => {
                    trace!("UI Automation CoCreateInstance failed: {:?}", e);
                    self.cached_state.store(false, Ordering::SeqCst);
                    return false;
                }
            };

            match automation.GetFocusedElement() {
                Ok(element) => match element.CurrentIsPassword() {
                    Ok(is_pwd) => {
                        let is_pwd_bool = is_pwd.as_bool();
                        if is_pwd_bool {
                            debug!("Active control is a password field (Privacy Guard active)");
                        }
                        self.cached_state.store(is_pwd_bool, Ordering::SeqCst);
                        is_pwd_bool
                    }
                    Err(e) => {
                        trace!("Failed to read CurrentIsPassword property: {:?}", e);
                        self.cached_state.store(false, Ordering::SeqCst);
                        false
                    }
                },
                Err(e) => {
                    trace!("No focused element returned from UI Automation: {:?}", e);
                    self.cached_state.store(false, Ordering::SeqCst);
                    false
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn check_focused_element_is_password(&self) -> bool {
        false
    }
}

impl Default for WindowsPrivacyGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyChecker for WindowsPrivacyGuard {
    fn is_password_field_active(&self) -> bool {
        self.check_focused_element_is_password()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_privacy_guard() {
        let guard = MockPrivacyGuard::new(false);
        assert!(!guard.is_password_field_active());

        guard.set_password_active(true);
        assert!(guard.is_password_field_active());

        guard.set_password_active(false);
        assert!(!guard.is_password_field_active());
    }

    #[test]
    fn test_windows_privacy_guard_instantiation() {
        let guard = WindowsPrivacyGuard::new();
        // Should execute safely without panicking even in headless/CI
        let _ = guard.is_password_field_active();
    }
}
