use crate::{get_engine_status, storage, AppState, Snippet};
use challa_core::{
    interactive, profiles::ProfileSettings, EngineEvent, PrivacyChecker, SnippetContent,
    TriggerMode,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
static UPDATING: AtomicBool = AtomicBool::new(false);

struct UpdateGuard;
impl Drop for UpdateGuard {
    fn drop(&mut self) {
        UPDATING.store(false, Ordering::SeqCst);
    }
}

pub struct TrayProfileMenu {
    pub work: tauri::menu::CheckMenuItem<Wry>,
    pub personal: tauri::menu::CheckMenuItem<Wry>,
}

pub fn update_tray_profile_checked(app: &tauri::AppHandle, active_profile: &str) {
    if let Some(menu) = app.try_state::<TrayProfileMenu>() {
        let _ = menu.work.set_checked(active_profile == "work");
        let _ = menu.personal.set_checked(active_profile == "personal");
    }
}

fn trigger_update_check(app: tauri::AppHandle) {
    if UPDATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let _guard = UpdateGuard;
        let updater = match app.updater() {
            Ok(u) => u,
            Err(e) => {
                app.dialog()
                    .message(format!("업데이트 확인에 실패했어요: {e}"))
                    .title("Challa 업데이트")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
                return;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let msg = format!(
                    "새 버전(v{})이 있어요.\n지금 설치하고 앱을 다시 시작할까요?",
                    update.version
                );
                let confirmed = app
                    .dialog()
                    .message(msg)
                    .title("Challa 업데이트")
                    .kind(MessageDialogKind::Info)
                    .buttons(MessageDialogButtons::YesNo)
                    .blocking_show();

                if confirmed {
                    app.state::<AppState>().bitwarden.lock();
                    match update.download_and_install(|_, _| {}, || {}).await {
                        Ok(()) => {
                            app.request_restart();
                        }
                        Err(e) => {
                            app.dialog()
                                .message(format!("업데이트 다운로드 및 설치에 실패했어요: {e}"))
                                .title("Challa 업데이트")
                                .kind(MessageDialogKind::Error)
                                .blocking_show();
                        }
                    }
                }
            }
            Ok(None) => {
                app.dialog()
                    .message("최신 버전입니다.")
                    .title("Challa 업데이트")
                    .kind(MessageDialogKind::Info)
                    .blocking_show();
            }
            Err(e) => {
                app.dialog()
                    .message(format!("업데이트 확인에 실패했어요: {e}"))
                    .title("Challa 업데이트")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
            }
        }
    });
}
#[derive(Clone)]
pub struct QuickRequest {
    token: u64,
    target: (usize, usize),
    pid: u32,
    expected: String,
    snippet_id: Option<String>,
    profile: String,
    created: Instant,
    vault_generation: u64,
}
#[derive(Serialize, PartialEq, Eq)]
pub struct ProductivityState {
    settings: ProfileSettings,
    effective_profile: String,
    application: String,
    readiness: HashMap<String, String>,
    shortcut_error: Option<String>,
}
fn snapshot(state: &AppState) -> Result<ProductivityState, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    let profile = state.profiles.get();
    let ready = state.bitwarden.prepared_references();
    let engine = get_engine_status_from(state);
    let matcher = state.matcher.lock().map_err(|e| e.to_string())?.clone();
    let mut readiness = HashMap::new();
    for snippet in matcher.list_snippets() {
        let reason = if !snippet.enabled {
            "disabled"
        } else if engine.1 {
            "paused"
        } else if !engine.0 {
            "engine_error"
        } else if snippet.profile != "common" && snippet.profile != profile {
            "profile"
        } else {
            match &snippet.content {
                SnippetContent::BitwardenRef { item_id, field } => {
                    if state.bitwarden.status() != challa_bitwarden::VaultStatus::Unlocked {
                        "vault_locked"
                    } else if !ready.contains(&challa_bitwarden::VaultReference {
                        item_id: item_id.clone(),
                        field: field.clone(),
                    }) {
                        "field_missing"
                    } else {
                        "ready"
                    }
                }
                SnippetContent::PlainText(text)
                    if snippet.template
                        && text.contains("{{")
                        && challa_core::template::fields(text).is_ok_and(|f| !f.is_empty()) =>
                {
                    "template"
                }
                _ => "ready",
            }
        };
        readiness.insert(snippet.id.clone(), reason.into());
    }
    Ok(ProductivityState {
        settings,
        effective_profile: profile.into(),
        application: interactive::application(interactive::current_target()),
        readiness,
        shortcut_error: state.shortcut_error.lock().ok().and_then(|e| e.clone()),
    })
}
fn get_engine_status_from(state: &AppState) -> (bool, bool) {
    state
        .hook
        .lock()
        .ok()
        .and_then(|h| h.as_ref().map(|h| (h.is_active(), h.is_paused())))
        .unwrap_or((false, false))
}
#[tauri::command]
pub fn get_productivity_state(state: State<'_, AppState>) -> Result<ProductivityState, String> {
    snapshot(&state)
}
#[tauri::command]
pub async fn save_profiles(app: tauri::AppHandle, settings: ProfileSettings) -> Result<(), String> {
    settings.validate()?;
    let active_profile = settings.active_profile.clone();
    let write_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = write_app.state::<AppState>();
        let _change = state.changes.lock().map_err(|e| e.to_string())?;
        storage::save_value(
            &state.storage_path.with_file_name("settings.dpapi"),
            &settings,
        )?;
        *state.settings.lock().map_err(|e| e.to_string())? = settings;
        state.profiles.set_target(0, "common");
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    update_tray_profile_checked(&app, &active_profile);
    let _ = app.emit("library-changed", ());
    Ok(())
}
#[tauri::command]
pub fn diagnose_snippet(
    state: State<'_, AppState>,
    snippet: Snippet,
) -> Result<Vec<String>, String> {
    let matcher = state.matcher.lock().map_err(|e| e.to_string())?.clone();
    Ok(conflicts(
        &snippet,
        &matcher
            .list_snippets()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
    ))
}
pub fn conflicts(draft: &Snippet, rows: &[Snippet]) -> Vec<String> {
    let mut messages = Vec::new();
    for other in rows.iter().filter(|s| {
        s.id != draft.id
            && (s.profile == draft.profile || s.profile == "common" || draft.profile == "common")
    }) {
        if other.keyword == draft.keyword
            && (other.trigger_mode == draft.trigger_mode
                || other.trigger_mode == TriggerMode::Delimiter
                || draft.trigger_mode == TriggerMode::Delimiter)
        {
            messages.push(format!("「{}」와 단축어 및 실행 조건이 겹쳐요", other.name));
            continue;
        }
        if !other.enabled || !draft.enabled {
            continue;
        }
        for (first, second) in [(other, draft), (draft, other)] {
            let matcher = challa_core::SnippetMatcher::from_snippets([first.clone()]);
            let mut typed = String::new();
            for ch in second.keyword.chars() {
                typed.push(ch);
                if matcher
                    .find_match_in_profile(&typed, &first.profile)
                    .is_some()
                {
                    messages.push(format!(
                        "「{}」 입력 중 「{}」가 먼저 실행될 수 있어요",
                        second.name, first.name
                    ));
                    break;
                }
            }
        }
    }
    messages.sort();
    messages.dedup();
    messages
}
#[derive(Serialize)]
pub struct QuickContext {
    token: u64,
    snippet_id: Option<String>,
    application: String,
    profile: String,
}
#[tauri::command]
pub fn get_quick_context(state: State<'_, AppState>) -> Result<QuickContext, String> {
    let request = state.quick.lock().map_err(|e| e.to_string())?;
    let request = request
        .as_ref()
        .ok_or("입력할 앱에서 Ctrl+Alt+Space를 눌러 주세요")?;
    Ok(QuickContext {
        token: request.token,
        snippet_id: request.snippet_id.clone(),
        application: interactive::application(request.target),
        profile: request.profile.clone(),
    })
}
fn open(
    app: &tauri::AppHandle,
    target: (usize, usize),
    snippet_id: Option<String>,
    expected: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let pid = interactive::process_id(target);
    if pid == 0
        || pid == std::process::id()
        || target.1 == 0
        || state
            .privacy
            .lock()
            .map_err(|e| e.to_string())?
            .is_input_blocked()
    {
        return Err("입력할 앱의 일반 입력창에서 실행해 주세요".into());
    }
    if !get_engine_status_from(&state).0 || get_engine_status_from(&state).1 {
        return Err("입력 엔진을 재개한 뒤 실행해 주세요".into());
    }
    let profile = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .effective(&interactive::application(target))
        .to_string();
    *state.quick.lock().map_err(|e| e.to_string())? = Some(QuickRequest {
        token: NEXT_REQUEST.fetch_add(1, Ordering::Relaxed),
        target,
        pid,
        expected,
        snippet_id,
        profile,
        created: Instant::now(),
        vault_generation: state.bitwarden.generation(),
    });
    let window = app
        .get_webview_window("quick")
        .ok_or("빠른 호출창을 열 수 없어요")?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    let _ = window.emit("quick-open", ());
    Ok(())
}
pub fn dismiss(app: &tauri::AppHandle, restore: bool) {
    let state = app.state::<AppState>();
    let request = state.quick.lock().ok().and_then(|mut r| r.take());
    if let Some(window) = app.get_webview_window("quick") {
        if request.is_some() {
            let _ = window.emit("quick-close", ());
        }
        let _ = window.hide();
    }
    if restore {
        if let Some(request) = request {
            let _ = interactive::restore(request.target, request.pid);
        }
    }
}
#[tauri::command]
pub fn cancel_quick(app: tauri::AppHandle) {
    dismiss(&app, true);
}
#[tauri::command]
pub async fn submit_quick(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    token: u64,
    snippet_id: String,
    values: BTreeMap<String, String>,
) -> Result<(), String> {
    let snippet = state
        .matcher
        .lock()
        .map_err(|e| e.to_string())?
        .get(&snippet_id)
        .cloned()
        .ok_or("스니펫을 다시 선택해 주세요")?;
    let request = {
        let guard = state.quick.lock().map_err(|e| e.to_string())?;
        let request = guard.as_ref().ok_or("빠른 호출을 다시 실행해 주세요")?;
        if request.token != token
            || request.created.elapsed() > Duration::from_secs(300)
            || request
                .snippet_id
                .as_ref()
                .is_some_and(|id| id != &snippet_id)
            || !snippet.enabled
            || (snippet.profile != "common" && snippet.profile != request.profile)
        {
            return Err("입력 요청 또는 프로필이 바뀌었어요. 다시 실행해 주세요".into());
        }
        request.clone()
    };
    let vault = state.bitwarden.clone();
    let privacy = state.privacy.lock().map_err(|e| e.to_string())?.clone();
    let paused = get_engine_status_from(&state).1;
    if paused {
        return Err("입력 엔진을 재개해 주세요".into());
    }
    if state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .effective(&interactive::application(request.target))
        != request.profile
    {
        return Err("작업 프로필이 바뀌었어요. 다시 실행해 주세요".into());
    }
    let window = app
        .get_webview_window("quick")
        .ok_or("빠른 호출창을 다시 열어 주세요")?;
    let generation = request.vault_generation;
    let text = match &snippet.content {
        SnippetContent::PlainText(text) => zeroize::Zeroizing::new(if snippet.template {
            challa_core::template::render(text, &values)?
        } else {
            text.clone()
        }),
        SnippetContent::BitwardenRef { item_id, field } => {
            if !vault.is_current(generation) {
                return Err("보관함 잠금을 해제한 뒤 빠른 호출을 다시 실행해 주세요".into());
            }
            vault
                .get_field(item_id, field)
                .map_err(|e| e.to_string())?
                .into_secret()
        }
    };
    if !window.is_focused().unwrap_or(false) {
        return Err("빠른 호출창이 닫혔어요. 다시 실행해 주세요".into());
    }
    {
        let mut current = state.quick.lock().map_err(|e| e.to_string())?;
        if current.as_ref().map(|r| r.token) != Some(token) {
            return Err("입력 요청이 취소됐어요".into());
        }
        current.take();
    }
    let _ = window.emit("quick-close", ());
    window.hide().map_err(|e| e.to_string())?;
    let injection_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        interactive::restore(request.target, request.pid).map_err(|e| e.to_string())?;
        if matches!(snippet.content, SnippetContent::BitwardenRef { .. })
            && !vault.is_current(generation)
        {
            return Err("보관함이 잠겼어요".into());
        }
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let done = std::sync::Arc::new(AtomicBool::new(false));
        let guard_cancelled = cancelled.clone();
        let guard_done = done.clone();
        let guard_vault = vault.clone();
        let target = request.target;
        let is_secret = matches!(snippet.content, SnippetContent::BitwardenRef { .. });
        let guard = std::thread::spawn(move || {
            while !guard_done.load(Ordering::Acquire) {
                let state = injection_app.state::<AppState>();
                if interactive::current_target() != target
                    || !get_engine_status_from(&state).0
                    || get_engine_status_from(&state).1
                    || (is_secret && !guard_vault.is_current(generation))
                {
                    guard_cancelled.store(true, Ordering::Release);
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let result = interactive::insert(
            &text,
            &request.expected,
            request.target,
            privacy.as_ref(),
            &cancelled,
        )
        .map_err(|e| e.to_string());
        done.store(true, Ordering::Release);
        let _ = guard.join();
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(error) = &result {
        let _ = app.emit("operation-error", error);
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.show();
        }
    }
    result
}
pub fn setup(app: tauri::AppHandle, receiver: Receiver<EngineEvent>) -> anyhow::Result<()> {
    tauri::WebviewWindowBuilder::new(
        &app,
        "quick",
        tauri::WebviewUrl::App("index.html#/quick".into()),
    )
    .title("Challa 빠른 호출")
    .inner_size(600.0, 520.0)
    .min_inner_size(360.0, 380.0)
    .visible(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()?;
    let tray = setup_tray(&app)
        .map_err(|_| "트레이 아이콘을 만들지 못했어요. 창을 닫으면 앱을 종료합니다".to_owned());
    crate::desktop::set_tray_result(&app, tray);
    let shortcut_app = app.clone();
    std::thread::spawn(move || unsafe {
        use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
        if RegisterHotKey(
            None,
            1,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            VK_SPACE.0 as u32,
        )
        .is_err()
        {
            if let Ok(mut error) = shortcut_app.state::<AppState>().shortcut_error.lock() {
                *error = Some("Ctrl+Alt+Space가 다른 앱에서 사용 중이에요".into());
            }
            let _ = shortcut_app.emit(
                "operation-error",
                "Ctrl+Alt+Space가 다른 앱에서 사용 중이에요",
            );
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if msg.message == WM_HOTKEY {
                let _ = open(
                    &shortcut_app,
                    interactive::current_target(),
                    None,
                    String::new(),
                )
                .map_err(|e| shortcut_app.emit("operation-error", e));
            }
        }
        let _ = UnregisterHotKey(None, 1);
    });
    let events_app = app.clone();
    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            if let EngineEvent::InteractiveExpansion {
                snippet_id,
                target,
                expected,
                revision,
            } = event
            {
                std::thread::sleep(Duration::from_millis(60));
                let state = events_app.state::<AppState>();
                let unchanged = state
                    .hook
                    .lock()
                    .ok()
                    .and_then(|h| h.as_ref().map(|h| h.input_revision() == revision))
                    .unwrap_or(false);
                if unchanged && interactive::current_target() == target {
                    let _ = open(&events_app, target, Some(snippet_id), expected);
                }
            }
            let _ = events_app.emit(
                "engine-state",
                get_engine_status(events_app.state::<AppState>()).ok(),
            );
        }
    });
    std::thread::spawn(move || {
        let mut previous = String::new();
        let mut last_pid = 0;
        let mut application = String::new();
        let mut previous_library = 0;
        loop {
            let state = app.state::<AppState>();
            let target = interactive::current_target();
            let pid = interactive::process_id(target);
            if pid != 0 && pid != std::process::id() {
                if pid != last_pid {
                    application = interactive::application(target);
                    last_pid = pid;
                }
            }
            if let Ok(settings) = state.settings.lock() {
                state
                    .profiles
                    .set_target(last_pid, settings.effective(&application));
            }
            let engine = get_engine_status(app.state::<AppState>()).ok();
            let revision = state
                .matcher
                .lock()
                .map(|m| m.revision())
                .unwrap_or_default();
            let ready = state.bitwarden.prepared_references();
            let settings = state.settings.lock().ok().map(|s| s.clone());
            let shortcut_error = state.shortcut_error.lock().ok().and_then(|e| e.clone());
            let serialized = serde_json::to_string(&(
                &engine,
                revision,
                &ready,
                &settings,
                state.profiles.get(),
                &application,
                shortcut_error,
            ))
            .unwrap_or_default();
            if serialized != previous {
                if let Ok(mut product) = snapshot(&state) {
                    product.application = application.clone();
                    previous = serialized;
                    let active_profile = settings
                        .as_ref()
                        .map(|s| s.active_profile.as_str())
                        .unwrap_or("work");
                    update_tray_profile_checked(&app, active_profile);
                    let _ = app.emit("engine-state", &engine);
                    let _ = app.emit("productivity-state", &product);
                    if revision != previous_library {
                        previous_library = revision;
                        let _ = app.emit("library-changed", ());
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
    Ok(())
}
fn setup_tray(app: &tauri::AppHandle) -> anyhow::Result<()> {
    use tauri::{
        menu::{CheckMenuItem, Menu, MenuItem, Submenu},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let initial_profile = app
        .state::<AppState>()
        .settings
        .lock()
        .map(|s| s.active_profile.clone())
        .unwrap_or_else(|_| "work".into());

    let update = MenuItem::with_id(app, "update", "업데이트", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "열기", true, None::<&str>)?;
    let work = CheckMenuItem::with_id(
        app,
        "work",
        "업무",
        true,
        initial_profile == "work",
        None::<&str>,
    )?;
    let personal = CheckMenuItem::with_id(
        app,
        "personal",
        "개인",
        true,
        initial_profile == "personal",
        None::<&str>,
    )?;
    let profile = Submenu::with_items(app, "프로필", true, &[&work, &personal])?;
    let pause = MenuItem::with_id(app, "pause", "일시 정지 / 재개", true, None::<&str>)?;
    let recover = MenuItem::with_id(app, "recover", "엔진 복구", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "보관함 잠금", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&update, &show, &profile, &pause, &recover, &lock, &quit],
    )?;

    app.manage(TrayProfileMenu {
        work: work.clone(),
        personal: personal.clone(),
    });

    TrayIconBuilder::with_id("challa-tray")
        .icon(tauri::include_image!("icons/icon.png"))
        .tooltip("Challa")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                crate::desktop::show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            let state = app.state::<AppState>();
            match event.id.as_ref() {
                "update" => {
                    trigger_update_check(app.clone());
                }
                "show" => {
                    crate::desktop::show_main(app);
                }
                "work" | "personal" => {
                    let _change = match state.changes.lock() {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let mut settings = match state.settings.lock() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut next = settings.clone();
                    next.active_profile = event.id.as_ref().into();
                    match storage::save_value(
                        &state.storage_path.with_file_name("settings.dpapi"),
                        &next,
                    ) {
                        Ok(()) => {
                            *settings = next.clone();
                            update_tray_profile_checked(app, &next.active_profile);
                        }
                        Err(error) => {
                            let _ = app.emit("operation-error", error);
                        }
                    }
                }
                "pause" => {
                    if let Ok(hook) = state.hook.lock() {
                        if let Some(hook) = hook.as_ref() {
                            hook.set_paused(!hook.is_paused());
                            if let Ok(mut b) = hook.buffer().lock() {
                                b.clear();
                            }
                        }
                    }
                }
                "recover" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match crate::recover_engine(app.clone()).await {
                            Ok(status) => {
                                let _ = app.emit("engine-state", status);
                            }
                            Err(error) => {
                                let _ = app.emit("operation-error", error);
                            }
                        }
                    });
                }
                "lock" => state.bitwarden.lock(),
                "quit" => {
                    crate::desktop::quit(app);
                }
                _ => (),
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflicts_include_prefix_suffix_and_overlapping_delimiters() {
        let short = Snippet::plain_text("1", "짧은 서명", ":sig", "a");
        let long = Snippet::plain_text("2", "긴 서명", ":signature", "b");
        assert!(!conflicts(&long, &[short.clone()]).is_empty());
        let mut isolated = long.clone();
        isolated.profile = "personal".into();
        let mut work = short.clone();
        work.profile = "work".into();
        assert!(conflicts(&isolated, &[work]).is_empty());
        let delimiter =
            Snippet::plain_text_with_trigger("3", "구분", ":sig", "c", TriggerMode::Delimiter);
        let space = Snippet::plain_text_with_trigger("4", "공백", ":sig", "d", TriggerMode::Space);
        assert!(!conflicts(&space, &[delimiter]).is_empty());
    }
}
