use challa_bitwarden::BitwardenClient;
use challa_core::{
    KeyboardHook, KeyboardHookHandle, Snippet, SnippetMatcher, TextInjector, TriggerMode,
};
use std::sync::{Arc, Mutex};
use tauri::State;
use tracing::{info, warn};

pub struct AppState {
    pub hook_handle: Mutex<Option<KeyboardHookHandle>>,
    pub matcher: Arc<Mutex<SnippetMatcher>>,
    pub injector: Arc<TextInjector>,
    pub bitwarden: Mutex<BitwardenClient>,
}

#[tauri::command]
fn get_hook_status(state: State<'_, AppState>) -> bool {
    let guard = state.hook_handle.lock().unwrap();
    guard.is_some()
}

#[tauri::command]
fn clear_buffer(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.hook_handle.lock().unwrap();
    if let Some(ref handle) = *guard {
        if let Ok(mut buf) = handle.buffer().lock() {
            buf.clear();
            return Ok(());
        }
    }
    Err("Hook not active or buffer locked".into())
}

#[tauri::command]
fn get_vault_status(state: State<'_, AppState>) -> String {
    let client = state.bitwarden.lock().unwrap();
    format!("{:?}", client.status())
}

#[tauri::command]
fn list_snippets(state: State<'_, AppState>) -> Vec<Snippet> {
    let matcher = state.matcher.lock().unwrap();
    matcher.list_snippets().into_iter().cloned().collect()
}

#[tauri::command]
fn add_snippet(state: State<'_, AppState>, snippet: Snippet) -> Result<(), String> {
    let mut matcher = state.matcher.lock().unwrap();
    matcher.add_snippet(snippet);
    Ok(())
}

#[tauri::command]
fn remove_snippet(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let mut matcher = state.matcher.lock().unwrap();
    Ok(matcher.remove_snippet(&id))
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting Challa application...");

    // Initialize Snippet Matcher with default welcome snippets
    let mut matcher = SnippetMatcher::new();
    matcher.add_snippet(Snippet::plain_text(
        "welcome_1",
        "Challa 소개 (콜론 접두사)",
        ":challa",
        "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더",
    ));
    matcher.add_snippet(Snippet::plain_text(
        "welcome_1_plain",
        "Challa 소개 (단어 직접 입력)",
        "challa",
        "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더",
    ));
    matcher.add_snippet(Snippet::plain_text_with_trigger(
        "welcome_2",
        "서명 템플릿 (콜론 접두사)",
        ":sig",
        "감사합니다.\nChalla 드림",
        TriggerMode::Space,
    ));
    matcher.add_snippet(Snippet::plain_text_with_trigger(
        "welcome_2_plain",
        "서명 템플릿 (단어 직접 입력)",
        "sig",
        "감사합니다.\nChalla 드림",
        TriggerMode::Space,
    ));

    let matcher_arc = Arc::new(Mutex::new(matcher));
    let injector_arc = Arc::new(TextInjector::new());

    // Initialize keyboard hook with matcher and injector
    let hook_result = KeyboardHook::new()
        .with_matcher(Arc::clone(&matcher_arc))
        .with_injector(Arc::clone(&injector_arc))
        .with_keyword_callback(|current| {
            info!("Keystroke captured, current buffer: {:?}", current);
        })
        .start();

    let hook_handle = match hook_result {
        Ok(handle) => {
            info!("Low-level keyboard hook active with Snippet Matcher and Injector");
            Some(handle)
        }
        Err(e) => {
            warn!("Failed to start low-level keyboard hook: {:?}", e);
            None
        }
    };

    let app_state = AppState {
        hook_handle: Mutex::new(hook_handle),
        matcher: matcher_arc,
        injector: injector_arc,
        bitwarden: Mutex::new(BitwardenClient::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_hook_status,
            clear_buffer,
            get_vault_status,
            list_snippets,
            add_snippet,
            remove_snippet
        ])
        .setup(|_app| {
            info!("Challa Tauri v2 setup completed");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running challa application");
}
