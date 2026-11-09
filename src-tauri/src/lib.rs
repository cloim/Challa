mod desktop;
mod productivity;
mod storage;
use challa_bitwarden::{BitwardenClient, VaultInfo, VaultItemSummary, VaultReference};
use challa_core::{KeyboardHook, KeyboardHookHandle, Snippet, SnippetMatcher, TriggerMode};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

pub struct AppState {
    hook: Mutex<Option<KeyboardHookHandle>>,
    settings: Mutex<challa_core::profiles::ProfileSettings>,
    profiles: Arc<challa_core::profiles::ProfileContext>,
    privacy: Arc<challa_core::WindowsPrivacyGuard>,
    quick: Mutex<Option<productivity::QuickRequest>>,
    shortcut_error: Mutex<Option<String>>,
    matcher: Arc<Mutex<SnippetMatcher>>,
    bitwarden: Arc<BitwardenClient>,
    changes: Arc<Mutex<()>>,
    storage_path: PathBuf,
    startup_error: Option<String>,
}
#[derive(Serialize, PartialEq, Eq, Clone)]
struct EngineStatus {
    active: bool,
    paused: bool,
    vault: String,
    error: Option<String>,
}
#[tauri::command]
fn get_engine_status(state: State<'_, AppState>) -> Result<EngineStatus, String> {
    let hook = state.hook.lock().map_err(|e| e.to_string())?;
    Ok(EngineStatus {
        active: hook.as_ref().is_some_and(|hook| hook.is_active()),
        paused: hook.as_ref().is_some_and(|hook| hook.is_paused()),
        vault: format!("{:?}", state.bitwarden.status()),
        error: state.startup_error.clone().or_else(|| {
            hook.as_ref()
                .and_then(|hook| hook.recent_error_message())
                .map(str::to_owned)
        }),
    })
}
#[tauri::command]
fn set_engine_paused(state: State<'_, AppState>, paused: bool) -> Result<(), String> {
    let guard = state.hook.lock().map_err(|e| e.to_string())?;
    let hook = guard.as_ref().ok_or("입력 엔진을 다시 시작해 주세요")?;
    hook.set_paused(paused);
    if let Ok(mut buffer) = hook.buffer().lock() {
        buffer.clear();
    }
    Ok(())
}
#[tauri::command]
fn clear_buffer(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.hook.lock().map_err(|e| e.to_string())?;
    let hook = guard.as_ref().ok_or("입력 엔진을 다시 시작해 주세요")?;
    hook.buffer().lock().map_err(|e| e.to_string())?.clear();
    Ok(())
}
#[tauri::command]
fn list_snippets(state: State<'_, AppState>) -> Result<Vec<Snippet>, String> {
    if let Some(error) = &state.startup_error {
        return Err(error.clone());
    }
    let matcher = state.matcher.lock().map_err(|e| e.to_string())?.clone();
    let mut snippets: Vec<_> = matcher.list_snippets().into_iter().cloned().collect();
    snippets.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(snippets)
}
fn validate(snippet: &Snippet, existing: &[Snippet]) -> Result<(), String> {
    if snippet.id.is_empty() || snippet.name.trim().is_empty() {
        return Err("스니펫 이름을 입력해 주세요".into());
    }
    if snippet.keyword.is_empty()
        || snippet.keyword.chars().count() > 64
        || !snippet.keyword.is_ascii()
        || snippet.keyword.chars().any(char::is_whitespace)
    {
        return Err("단축어는 공백 없이 영문, 숫자와 기호 1~64자로 입력해 주세요".into());
    }
    if existing.iter().any(|other| {
        other.id != snippet.id
            && other.keyword == snippet.keyword
            && other.trigger_mode == snippet.trigger_mode
            && (other.profile == snippet.profile
                || other.profile == "common"
                || snippet.profile == "common")
    }) {
        return Err("같은 단축어와 실행 방식이 이미 등록되어 있어요".into());
    }
    if !matches!(snippet.profile.as_str(), "common" | "work" | "personal") {
        return Err("스니펫 프로필을 선택해 주세요".into());
    }
    if let challa_core::SnippetContent::PlainText(text) = &snippet.content {
        if snippet.template {
            challa_core::template::fields(text)?;
        }
        if text.is_empty() || text.encode_utf16().count() > 16_384 || text.contains('\0') {
            return Err("본문은 1~16,384자로 입력해 주세요".into());
        }
    }
    if let challa_core::SnippetContent::BitwardenRef { item_id, field } = &snippet.content {
        if snippet.template {
            return Err("빈칸 템플릿은 일반 문구에 사용할 수 있어요".into());
        }
        if !challa_bitwarden::client::valid_item_id(item_id)
            || matches!(field, challa_bitwarden::BitwardenField::Custom(name) if name.trim().is_empty() || name.len() > 256)
        {
            return Err("Bitwarden 항목과 필드를 선택해 주세요".into());
        }
    }
    Ok(())
}
fn references(snippets: &[Snippet]) -> Vec<VaultReference> {
    snippets
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| match &s.content {
            challa_core::SnippetContent::BitwardenRef { item_id, field } => Some(VaultReference {
                item_id: item_id.clone(),
                field: field.clone(),
            }),
            _ => None,
        })
        .collect()
}
#[tauri::command]
fn get_vault_status(state: State<'_, AppState>) -> VaultInfo {
    state.bitwarden.info()
}
#[tauri::command]
fn lock_vault(state: State<'_, AppState>) {
    state.bitwarden.lock();
}
#[tauri::command]
async fn logout_vault(state: State<'_, AppState>) -> Result<VaultInfo, String> {
    state.bitwarden.lock();
    let vault = Arc::clone(&state.bitwarden);
    tauri::async_runtime::spawn_blocking(move || vault.logout().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn inspect_vault(state: State<'_, AppState>) -> Result<VaultInfo, String> {
    let vault = Arc::clone(&state.bitwarden);
    tauri::async_runtime::spawn_blocking(move || vault.inspect().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn login_vault(
    state: State<'_, AppState>,
    server: String,
    client_id: String,
    client_secret: String,
) -> Result<VaultInfo, String> {
    let vault = Arc::clone(&state.bitwarden);
    let secret = zeroize::Zeroizing::new(client_secret);
    tauri::async_runtime::spawn_blocking(move || {
        vault
            .login(&server, &client_id, &secret)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn unlock_vault(state: State<'_, AppState>, password: String) -> Result<VaultInfo, String> {
    let vault = Arc::clone(&state.bitwarden);
    let matcher = Arc::clone(&state.matcher);
    let password = zeroize::Zeroizing::new(password);
    let changes = Arc::clone(&state.changes);
    tauri::async_runtime::spawn_blocking(move || {
        let _change = changes.lock().map_err(|e| e.to_string())?;
        vault.unlock(&password).map_err(|e| e.to_string())?;
        let refs = references(
            &matcher
                .lock()
                .map_err(|e| e.to_string())?
                .list_snippets()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        );
        vault.prepare(&refs).map_err(|e| e.to_string())?;
        Ok(vault.info())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn list_vault_items(state: State<'_, AppState>) -> Result<Vec<VaultItemSummary>, String> {
    let vault = Arc::clone(&state.bitwarden);
    tauri::async_runtime::spawn_blocking(move || vault.list_items().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn sync_vault(state: State<'_, AppState>) -> Result<VaultInfo, String> {
    let vault = Arc::clone(&state.bitwarden);
    let matcher = Arc::clone(&state.matcher);
    let changes = Arc::clone(&state.changes);
    tauri::async_runtime::spawn_blocking(move || {
        let _change = changes.lock().map_err(|e| e.to_string())?;
        vault.sync().map_err(|e| e.to_string())?;
        let refs = references(
            &matcher
                .lock()
                .map_err(|e| e.to_string())?
                .list_snippets()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        );
        vault.prepare(&refs).map_err(|e| e.to_string())?;
        Ok(vault.info())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn add_snippet(state: State<'_, AppState>, snippet: Snippet) -> Result<(), String> {
    if let Some(error) = &state.startup_error {
        return Err(error.clone());
    }
    let matcher = Arc::clone(&state.matcher);
    let path = state.storage_path.clone();
    let vault = Arc::clone(&state.bitwarden);
    let changes = Arc::clone(&state.changes);
    tauri::async_runtime::spawn_blocking(move || {
        let _change = changes.lock().map_err(|e| e.to_string())?;
        let snapshot = matcher.lock().map_err(|e| e.to_string())?.clone();
        let mut snippets: Vec<_> = snapshot.list_snippets().into_iter().cloned().collect();
        validate(&snippet, &snippets)?;
        let old_refs = references(&snippets);
        snippets.retain(|other| other.id != snippet.id);
        snippets.push(snippet.clone());
        let new_refs = references(&snippets);
        if new_refs.iter().any(|r| !old_refs.contains(r)) {
            vault.prepare(&new_refs).map_err(|e| e.to_string())?;
        }
        let replacement = SnippetMatcher::from_snippets(snippets.clone());
        storage::save(&path, &snippets)?;
        let previous = std::mem::replace(
            &mut *matcher.lock().map_err(|e| e.to_string())?,
            replacement,
        );
        drop(previous);
        vault.retain_references(&new_refs);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn remove_snippet(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    if let Some(error) = &state.startup_error {
        return Err(error.clone());
    }
    let matcher = Arc::clone(&state.matcher);
    let path = state.storage_path.clone();
    let vault = Arc::clone(&state.bitwarden);
    let changes = Arc::clone(&state.changes);
    tauri::async_runtime::spawn_blocking(move || {
        let _change = changes.lock().map_err(|e| e.to_string())?;
        let snapshot = matcher.lock().map_err(|e| e.to_string())?.clone();
        let found = snapshot.get(&id).is_some();
        let snippets: Vec<_> = snapshot
            .list_snippets()
            .into_iter()
            .filter(|s| s.id != id)
            .cloned()
            .collect();
        let replacement = SnippetMatcher::from_snippets(snippets.clone());
        storage::save(&path, &snippets)?;
        let previous = std::mem::replace(
            &mut *matcher.lock().map_err(|e| e.to_string())?,
            replacement,
        );
        drop(previous);
        vault.retain_references(&references(&snippets));
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();
    let mut builder = tauri::Builder::default();
    // Isolated native smoke tests never open the person's saved snippets or vault.
    let test_directory = if cfg!(debug_assertions) {
        std::env::var_os("CHALLA_TEST_DATA_DIR").map(PathBuf::from)
    } else {
        None
    };
    if test_directory.is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|arg| arg == "--background") {
                desktop::show_main(app);
            }
        }));
    }
    builder
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--background"])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            desktop::get_desktop_settings,
            desktop::save_desktop_settings,
            desktop::hide_to_tray,
            productivity::get_productivity_state,
            productivity::save_profiles,
            productivity::diagnose_snippet,
            productivity::get_quick_context,
            productivity::submit_quick,
            productivity::cancel_quick,
            get_vault_status,
            lock_vault,
            logout_vault,
            inspect_vault,
            login_vault,
            unlock_vault,
            list_vault_items,
            sync_vault,
            get_engine_status,
            set_engine_paused,
            clear_buffer,
            list_snippets,
            add_snippet,
            remove_snippet
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_icon(tauri::include_image!("icons/icon.png"))?;
            }
            let data_directory = test_directory
                .clone()
                .unwrap_or(app.path().app_local_data_dir()?);
            if !data_directory.is_absolute() {
                return Err("앱 데이터 경로는 절대 경로여야 합니다".into());
            }
            let storage_path = data_directory.join("snippets.dpapi");
            desktop::setup(app.handle(), &data_directory, test_directory.is_some());
            let bitwarden = Arc::new(BitwardenClient::with_profile(
                data_directory.join("bitwarden-cli"),
            ));
            // The vault stays unlocked while the person works and locks when Windows locks.
            #[cfg(windows)]
            {
                let weak_vault = Arc::downgrade(&bitwarden);
                std::thread::spawn(move || {
                    let mut session_locked = false;
                    while let Some(vault) = weak_vault.upgrade() {
                        let locked = challa_bitwarden::windows_session_locked();
                        if locked && !session_locked {
                            vault.lock();
                        }
                        session_locked = locked;
                        drop(vault);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                });
            }
            let (snippets, startup_error) = match storage::load(&storage_path) {
                Ok(Some(snippets)) => (snippets, None),
                Ok(None) => (
                    vec![
                        Snippet::plain_text(
                            "welcome_1",
                            "Challa 소개",
                            ":challa",
                            "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더",
                        ),
                        Snippet::plain_text_with_trigger(
                            "welcome_2",
                            "서명 템플릿",
                            ":sig",
                            "감사합니다.\nChalla 드림",
                            TriggerMode::Space,
                        ),
                    ],
                    None,
                ),
                Err(error) => (vec![], Some(error)),
            };
            let settings: challa_core::profiles::ProfileSettings =
                storage::load_value(&storage_path.with_file_name("settings.dpapi"))?
                    .unwrap_or_default();
            settings.validate()?;
            let profiles = Arc::new(challa_core::profiles::ProfileContext::default());
            let privacy = Arc::new(challa_core::WindowsPrivacyGuard::new());
            let (events_tx, events_rx) = std::sync::mpsc::channel();
            let matcher = Arc::new(Mutex::new(SnippetMatcher::from_snippets(snippets)));
            let (hook, hook_error) = match KeyboardHook::new()
                .with_matcher(Arc::clone(&matcher))
                .with_bitwarden(Arc::clone(&bitwarden))
                .with_privacy_guard(privacy.clone())
                .with_profiles(Arc::clone(&profiles))
                .with_event_sender(events_tx)
                .start()
            {
                Ok(hook) => (Some(hook), None),
                Err(error) => (None, Some(format!("입력 엔진 시작 오류: {error}"))),
            };
            app.manage(AppState {
                settings: Mutex::new(settings),
                profiles,
                privacy,
                quick: Mutex::new(None),
                shortcut_error: Mutex::new(None),
                hook: Mutex::new(hook),
                matcher,
                bitwarden,
                changes: Arc::new(Mutex::new(())),
                storage_path,
                startup_error: startup_error.or(hook_error),
            });
            productivity::setup(app.handle().clone(), events_rx)?;
            desktop::show_on_launch(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "quick" {
                    api.prevent_close();
                    productivity::dismiss(window.app_handle(), true);
                    let _ = window.hide();
                } else if window.label() == "main" {
                    if desktop::close_to_tray(window.app_handle()) {
                        api.prevent_close();
                        let _ = window.hide();
                    } else {
                        desktop::quit(window.app_handle());
                    }
                }
            }
            if window.label() == "quick" && matches!(event, tauri::WindowEvent::Focused(false)) {
                productivity::dismiss(window.app_handle(), false);
            }
        })
        .run(tauri::generate_context!())
        .expect("Challa application could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_and_ime_composition_keywords_are_rejected_before_saving() {
        let snippet = Snippet::plain_text("1", "소개", ":challa", "한글");
        let duplicate = Snippet::plain_text("2", "중복", ":challa", "다른 내용");
        assert!(validate(&duplicate, &[snippet]).is_err());
        assert!(validate(&Snippet::plain_text("3", "조합", "한글", "본문"), &[]).is_err());
    }
}
