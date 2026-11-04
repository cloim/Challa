use challa_bitwarden::BitwardenClient;
use challa_core::{KeyboardHook, KeyboardHookHandle};
use std::sync::Mutex;
use tauri::State;
use tracing::{info, warn};

pub struct AppState {
    pub hook_handle: Mutex<Option<KeyboardHookHandle>>,
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

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting Challa application...");

    // Initialize keyboard hook
    let hook_result = KeyboardHook::new()
        .with_keyword_callback(|current| {
            info!("Keystroke captured, current buffer: {}", current);
        })
        .start();

    let hook_handle = match hook_result {
        Ok(handle) => {
            info!("Low-level keyboard hook active");
            Some(handle)
        }
        Err(e) => {
            warn!("Failed to start low-level keyboard hook: {:?}", e);
            None
        }
    };

    let app_state = AppState {
        hook_handle: Mutex::new(hook_handle),
        bitwarden: Mutex::new(BitwardenClient::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_hook_status,
            clear_buffer,
            get_vault_status
        ])
        .setup(|_app| {
            info!("Challa Tauri v2 setup completed");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running challa application");
}
