use crate::{storage, AppState};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{Manager, State};
use tauri_plugin_autostart::ManagerExt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DesktopPreferences {
    pub start_with_windows: bool,
    pub close_to_tray: bool,
}
impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            start_with_windows: true,
            close_to_tray: true,
        }
    }
}
pub struct DesktopState {
    preferences: Mutex<DesktopPreferences>,
    path: PathBuf,
    startup_supported: bool,
    tray_available: AtomicBool,
    error: Mutex<Option<String>>,
}
#[derive(Serialize)]
pub struct DesktopStatus {
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub startup_supported: bool,
    pub tray_available: bool,
    pub error: Option<String>,
}
pub fn setup(app: &tauri::AppHandle, directory: &Path, isolated: bool) {
    let path = directory.join("desktop.dpapi");
    let (preferences, mut error) = match storage::load_value::<DesktopPreferences>(&path) {
        Ok(preferences) => (preferences.unwrap_or_default(), None),
        Err(error) => (
            DesktopPreferences {
                start_with_windows: false,
                close_to_tray: true,
            },
            Some(error),
        ),
    };
    // Development and isolated test instances must not replace the release
    // executable registered in the person's Windows startup settings.
    let startup_supported = cfg!(windows) && !cfg!(debug_assertions) && !isolated;
    if startup_supported && error.is_none() {
        if let Err(problem) = apply_autostart(app, preferences.start_with_windows) {
            error = Some(problem);
        }
    }
    app.manage(DesktopState {
        preferences: Mutex::new(preferences),
        path,
        startup_supported,
        tray_available: AtomicBool::new(false),
        error: Mutex::new(error),
    });
}
fn apply_autostart(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|_| "Windows 자동 실행 설정을 변경하지 못했어요".into())
}
pub fn set_tray_result(app: &tauri::AppHandle, result: Result<(), String>) {
    let state = app.state::<DesktopState>();
    state
        .tray_available
        .store(result.is_ok(), Ordering::Release);
    if let Err(error) = result {
        if let Ok(mut slot) = state.error.lock() {
            *slot = Some(error);
        }
    }
}
pub fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
pub fn close_to_tray(app: &tauri::AppHandle) -> bool {
    let state = app.state::<DesktopState>();
    state.tray_available.load(Ordering::Acquire)
        && state
            .preferences
            .lock()
            .map(|p| p.close_to_tray)
            .unwrap_or(false)
}
pub fn quit(app: &tauri::AppHandle) {
    app.state::<AppState>().bitwarden.lock();
    app.exit(0);
}
pub fn show_on_launch(app: &tauri::AppHandle) {
    let tray_ready = app
        .state::<DesktopState>()
        .tray_available
        .load(Ordering::Acquire);
    if !std::env::args().any(|arg| arg == "--background") || !tray_ready {
        show_main(app);
    }
}
#[tauri::command]
pub fn get_desktop_settings(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopStatus, String> {
    let preferences = state.preferences.lock().map_err(|e| e.to_string())?;
    let mut error = state.error.lock().map_err(|e| e.to_string())?.clone();
    let start_with_windows = if state.startup_supported {
        match app.autolaunch().is_enabled() {
            Ok(enabled) => enabled,
            Err(_) => {
                error = Some("Windows 자동 실행 상태를 확인하지 못했어요".into());
                false
            }
        }
    } else {
        false
    };
    Ok(DesktopStatus {
        start_with_windows,
        close_to_tray: preferences.close_to_tray,
        startup_supported: state.startup_supported,
        tray_available: state.tray_available.load(Ordering::Acquire),
        error,
    })
}
fn persist_preferences(
    path: &Path,
    previous: &DesktopPreferences,
    next: &DesktopPreferences,
    startup_supported: bool,
    apply: impl Fn(bool) -> Result<(), String>,
) -> Result<(), String> {
    let changed = startup_supported && previous.start_with_windows != next.start_with_windows;
    if changed {
        apply(next.start_with_windows)?;
    }
    if let Err(error) = storage::save_value(path, next) {
        if changed {
            apply(previous.start_with_windows).map_err(|_| {
                "설정 저장과 자동 실행 복구가 실패했어요. 설정을 다시 확인해 주세요".to_owned()
            })?;
        }
        return Err(error);
    }
    Ok(())
}
#[tauri::command]
pub async fn save_desktop_settings(
    app: tauri::AppHandle,
    mut preferences: DesktopPreferences,
) -> Result<DesktopStatus, String> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = worker.state::<DesktopState>();
        let mut current = state.preferences.lock().map_err(|e| e.to_string())?;
        let mut previous = current.clone();
        if state.startup_supported {
            previous.start_with_windows = worker
                .autolaunch()
                .is_enabled()
                .map_err(|_| "자동 실행 상태를 확인하지 못했어요".to_owned())?;
        } else {
            preferences.start_with_windows = current.start_with_windows;
        }
        persist_preferences(
            &state.path,
            &previous,
            &preferences,
            state.startup_supported,
            |enabled| apply_autostart(&worker, enabled),
        )?;
        *current = preferences;
        *state.error.lock().map_err(|e| e.to_string())? = None;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    get_desktop_settings(app.clone(), app.state::<DesktopState>())
}
#[tauri::command]
pub fn hide_to_tray(app: tauri::AppHandle) -> Result<(), String> {
    if !app
        .state::<DesktopState>()
        .tray_available
        .load(Ordering::Acquire)
    {
        return Err("트레이 아이콘을 확인하지 못했어요".into());
    }
    app.get_webview_window("main")
        .ok_or("관리 창을 찾지 못했어요")?
        .hide()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn failed_preference_save_restores_the_previous_startup_setting() {
        let blocker =
            std::env::temp_dir().join(format!("challa-desktop-blocker-{}", std::process::id()));
        std::fs::write(&blocker, b"file").unwrap();
        let previous = DesktopPreferences::default();
        let next = DesktopPreferences {
            start_with_windows: false,
            close_to_tray: false,
        };
        let calls = RefCell::new(Vec::new());
        let result = persist_preferences(
            &blocker.join("desktop.dpapi"),
            &previous,
            &next,
            true,
            |enabled| {
                calls.borrow_mut().push(enabled);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(*calls.borrow(), vec![false, true]);
        std::fs::remove_file(blocker).unwrap();
    }
    #[test]
    fn preference_changes_survive_restart_without_mutating_os_settings_in_tests() {
        let path = std::env::temp_dir().join(format!(
            "challa-desktop-preferences-{}.dpapi",
            std::process::id()
        ));
        let next = DesktopPreferences {
            start_with_windows: false,
            close_to_tray: false,
        };
        persist_preferences(&path, &DesktopPreferences::default(), &next, false, |_| {
            panic!("isolated settings must not change Windows startup")
        })
        .unwrap();
        assert_eq!(
            storage::load_value::<DesktopPreferences>(&path).unwrap(),
            Some(next)
        );
        std::fs::remove_file(path).unwrap();
    }
}
