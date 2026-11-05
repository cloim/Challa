//! Debug-only native integration harness. The caller owns the foreground test window.
use challa_core::{KeyboardHook, Snippet, SnippetMatcher, TriggerMode};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
fn main() -> anyhow::Result<()> {
    if !cfg!(debug_assertions) {
        anyhow::bail!("Use a debug build for marked synthetic input tests");
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();
    let args: Vec<String> = std::env::args().collect();
    let ready = args.get(1).expect("ready file path");
    let stop = args.get(2).expect("stop file path");
    let mut matcher = SnippetMatcher::new();
    matcher.add_snippet(Snippet::plain_text(
        "1",
        "소개",
        ":challa",
        "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더",
    ));
    matcher.add_snippet(Snippet::plain_text_with_trigger(
        "2",
        "서명",
        ":sig",
        "감사합니다.\nChalla 드림",
        TriggerMode::Space,
    ));
    matcher.add_snippet(Snippet::plain_text("3", "이모지", ":emoji", "안녕 😀"));
    matcher.add_snippet(Snippet::plain_text_with_trigger(
        "4",
        "Enter",
        ":enter",
        "엔터 완료",
        TriggerMode::Enter,
    ));
    matcher.add_snippet(Snippet::plain_text_with_trigger(
        "5",
        "Tab",
        ":tab",
        "탭 완료",
        TriggerMode::Tab,
    ));
    matcher.add_snippet(Snippet::plain_text("6", "Shift", ":UP", "대문자 치환"));
    matcher.add_snippet(Snippet::plain_text(
        "7",
        "Clipboard",
        ":long",
        "안전한 긴 문장입니다.\n".repeat(30),
    ));
    let mut hook = KeyboardHook::new()
        .with_matcher(Arc::new(Mutex::new(matcher)))
        .start()?;
    if args.iter().any(|arg| arg == "--paused") {
        hook.set_paused(true);
    }
    std::fs::write(ready, "ready")?;
    let started = Instant::now();
    while !std::path::Path::new(stop).exists() && started.elapsed() < Duration::from_secs(180) {
        std::thread::sleep(Duration::from_millis(10));
    }
    hook.stop();
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"metrics": hook.metrics(), "buffer_empty": hook.buffer().lock().unwrap().is_empty()})
        )?
    );
    Ok(())
}
