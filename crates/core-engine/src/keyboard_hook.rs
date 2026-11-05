use crate::events::EngineEvent;
use crate::{PrivacyChecker, SecureRingBuffer, SnippetMatcher, WindowsPrivacyGuard};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Default)]
struct Metrics {
    input_revision: AtomicU64,
    callbacks: AtomicU64,
    max_callback_us: AtomicU64,
    substitutions: AtomicU64,
    errors: AtomicU64,
    recent_error: AtomicBool,
    recent_error_kind: AtomicUsize,
    worker_window: AtomicUsize,
}
#[derive(Debug, serde::Serialize)]
pub struct EngineMetrics {
    pub callbacks: u64,
    pub max_callback_us: u64,
    pub substitutions: u64,
    pub errors: u64,
}

pub struct KeyboardHookHandle {
    thread_handle: Option<JoinHandle<()>>,
    thread_id: u32,
    buffer: Arc<Mutex<SecureRingBuffer>>,
    paused: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    metrics: Arc<Metrics>,
}
impl KeyboardHookHandle {
    pub fn input_revision(&self) -> u64 {
        self.metrics.input_revision.load(Ordering::Acquire)
    }
    pub fn buffer(&self) -> Arc<Mutex<SecureRingBuffer>> {
        Arc::clone(&self.buffer)
    }
    pub fn is_active(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub fn has_recent_error(&self) -> bool {
        self.metrics.recent_error.load(Ordering::Acquire)
    }
    pub fn recent_error_message(&self) -> Option<&'static str> {
        self.has_recent_error().then(|| {
            match self.metrics.recent_error_kind.load(Ordering::Acquire) {
                1 => "한글 입력 모드를 확인하거나 정리하지 못했어요",
                2 => "입력창에서 단축어 위치를 확인하지 못했어요",
                3 => "입력 위치가 바뀌어 치환을 중단했어요",
                4 => "Windows가 치환 입력을 거절했어요",
                5 => "클립보드 상태를 확인해 주세요",
                6 => "치환 응답 시간이 초과됐어요",
                _ => "치환하지 못했어요. 입력 위치를 확인해 주세요",
            }
        })
    }
    pub fn metrics(&self) -> EngineMetrics {
        EngineMetrics {
            callbacks: self.metrics.callbacks.load(Ordering::Relaxed),
            max_callback_us: self.metrics.max_callback_us.load(Ordering::Relaxed),
            substitutions: self.metrics.substitutions.load(Ordering::Relaxed),
            errors: self.metrics.errors.load(Ordering::Relaxed),
        }
    }
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }
    pub fn stop(&mut self) {
        #[cfg(target_os = "windows")]
        if self.thread_id != 0 {
            use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            self.thread_id = 0;
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
fn substitution_error_kind(message: &str) -> usize {
    if message.contains("Korean") {
        1
    } else if message.contains("Typed keyword")
        || message.contains("caret")
        || message.contains("selection")
    {
        2
    } else if message.contains("focus") || message.contains("cancelled") {
        3
    } else if message.contains("SendInput") {
        4
    } else if message.contains("Clipboard") || message.contains("clipboard") {
        5
    } else {
        0
    }
}
impl Drop for KeyboardHookHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct KeyboardHook {
    profiles: Option<Arc<crate::profiles::ProfileContext>>,
    buffer: Arc<Mutex<SecureRingBuffer>>,
    privacy_guard: Arc<dyn PrivacyChecker>,
    event_sender: Option<Sender<EngineEvent>>,
    matcher: Option<Arc<Mutex<SnippetMatcher>>>,
    bitwarden: Option<Arc<challa_bitwarden::BitwardenClient>>,
    capture_own_process: bool,
    metrics: Arc<Metrics>,
}
impl KeyboardHook {
    pub fn new() -> Self {
        Self {
            profiles: None,
            buffer: Arc::new(Mutex::new(SecureRingBuffer::with_capacity_and_timeout(
                256,
                crate::DEFAULT_IDLE_TIMEOUT,
            ))),
            privacy_guard: Arc::new(WindowsPrivacyGuard::new()),
            event_sender: None,
            matcher: None,
            bitwarden: None,
            capture_own_process: false,
            metrics: Arc::new(Metrics::default()),
        }
    }
    pub fn with_buffer(mut self, buffer: Arc<Mutex<SecureRingBuffer>>) -> Self {
        self.buffer = buffer;
        self
    }
    pub fn with_profiles(mut self, profiles: Arc<crate::profiles::ProfileContext>) -> Self {
        self.profiles = Some(profiles);
        self
    }
    pub fn with_privacy_guard(mut self, guard: Arc<dyn PrivacyChecker>) -> Self {
        self.privacy_guard = guard;
        self
    }
    pub fn with_event_sender(mut self, sender: Sender<EngineEvent>) -> Self {
        self.event_sender = Some(sender);
        self
    }
    pub fn with_matcher(mut self, matcher: Arc<Mutex<SnippetMatcher>>) -> Self {
        self.matcher = Some(matcher);
        self
    }
    pub fn with_bitwarden(mut self, client: Arc<challa_bitwarden::BitwardenClient>) -> Self {
        self.bitwarden = Some(client);
        self
    }
    /// Only test harnesses opt into capturing their own edit controls.
    pub fn with_own_process_capture(mut self, enabled: bool) -> Self {
        self.capture_own_process = enabled;
        self
    }

    pub fn start(self) -> anyhow::Result<KeyboardHookHandle> {
        #[cfg(not(target_os = "windows"))]
        anyhow::bail!("The global keyboard engine requires Windows");
        #[cfg(target_os = "windows")]
        {
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
            let buffer = Arc::clone(&self.buffer);
            let metrics = Arc::clone(&self.metrics);
            let paused = Arc::new(AtomicBool::new(false));
            let alive = Arc::new(AtomicBool::new(false));
            let hook_paused = Arc::clone(&paused);
            let hook_alive = Arc::clone(&alive);
            let thread_handle = std::thread::spawn(move || {
                windows_impl::run(self, hook_paused, hook_alive, ready_tx)
            });
            let thread_id = ready_rx.recv_timeout(std::time::Duration::from_secs(3))??;
            Ok(KeyboardHookHandle {
                thread_handle: Some(thread_handle),
                thread_id,
                buffer,
                paused,
                alive,
                metrics,
            })
        }
    }
}
impl Default for KeyboardHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use crate::injector::*;
    use crate::privacy_guard::input_target;
    use crate::SnippetContent;
    use std::cell::RefCell;
    use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::Input::Ime::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use zeroize::Zeroizing;

    const FAILED: u32 = WM_APP + 1;
    const REPLAY: u32 = WM_APP + 2;
    // Explicit marker is used only by debug-build native regression tests.
    const TEST_TAG: usize = 0x43485453;
    struct Task {
        id: usize,
        text: Zeroizing<String>,
        vault_generation: Option<u64>,
        erase: usize,
        modifiers: Vec<VIRTUAL_KEY>,
        target: (usize, usize),
        keyword: String,
        expected: String,
        password: bool,
        cancelled: Arc<AtomicBool>,
    }
    struct Pending {
        id: usize,
        keyboard: [u8; 256],
        trigger: INPUT,
        trigger_released: bool,
        keys: Vec<INPUT>,
        target: (usize, usize),
        started: Instant,
        cancelled: Arc<AtomicBool>,
    }
    struct Context {
        hook: KeyboardHook,
        paused: Arc<AtomicBool>,
        keyboard: [u8; 256],
        target: (usize, usize),
        pending: Option<Pending>,
        tasks: SyncSender<Task>,
        next_id: usize,
        replay: Vec<INPUT>,
        suppressed_up: [bool; 256],
        busy: Arc<AtomicBool>,
        replay_depth: usize,
        spool: Sender<((usize, usize), Vec<INPUT>)>,
    }
    thread_local! { static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) }; }

    impl Context {
        fn clear(&self) {
            if let Ok(mut buffer) = self.hook.buffer.try_lock() {
                buffer.clear();
            }
        }
        fn finish(&mut self, failed: bool) {
            let Some(pending) = self.pending.take() else {
                return;
            };
            self.busy.store(false, Ordering::Release);
            if failed {
                pending.cancelled.store(true, Ordering::Release);
            }
            self.clear();
            // A new foreground window must never receive replay belonging to the old one.
            if input_target() != pending.target {
                return;
            }
            let mut keys = pending.keys;
            if failed {
                let vk = unsafe { pending.trigger.Anonymous.ki.wVk };
                self.suppressed_up[vk.0 as usize] = false;
                if pending.trigger_released {
                    keys.insert(0, key_input(vk, 0, KEYEVENTF_KEYUP, INPUT_TAG));
                }
                keys.insert(0, pending.trigger);
            }
            self.keyboard = pending.keyboard;
            self.replay.extend(keys);
            // SendInput can synchronously reenter the hook. Dispatch only after the
            // callback and its RefCell borrow have returned to the message pump.
            unsafe {
                let _ = PostThreadMessageW(GetCurrentThreadId(), REPLAY, WPARAM(0), LPARAM(0));
            }
        }
        fn modifiers(&self) -> Vec<VIRTUAL_KEY> {
            MODIFIERS
                .iter()
                .copied()
                .filter(|k| self.keyboard[k.0 as usize] & 0x80 != 0)
                .collect()
        }
        fn update_key(&mut self, vk: usize, down: bool) {
            if vk >= 256 {
                return;
            }
            self.keyboard[vk] = (self.keyboard[vk] & 1) | if down { 0x80 } else { 0 };
            for (generic, left, right) in [
                (VK_SHIFT, VK_LSHIFT, VK_RSHIFT),
                (VK_CONTROL, VK_LCONTROL, VK_RCONTROL),
                (VK_MENU, VK_LMENU, VK_RMENU),
            ] {
                self.keyboard[generic.0 as usize] =
                    self.keyboard[left.0 as usize] | self.keyboard[right.0 as usize];
            }
            if down && vk == VK_CAPITAL.0 as usize {
                self.keyboard[vk] ^= 1;
            }
        }
        fn process(&mut self, key: &KBDLLHOOKSTRUCT, down: bool) -> bool {
            if down && key.dwExtraInfo != INPUT_TAG && key.dwExtraInfo != REPLAY_TAG {
                self.hook
                    .metrics
                    .input_revision
                    .fetch_add(1, Ordering::AcqRel);
            }
            let current_target = input_target();
            if current_target != self.target {
                if let Some(pending) = &self.pending {
                    pending.cancelled.store(true, Ordering::Release);
                }
                self.finish(true);
                self.replay.clear();
                self.clear();
                self.target = current_target;
                if key.dwExtraInfo == REPLAY_TAG {
                    return true;
                }
            }
            if self.paused.load(Ordering::Acquire) || self.hook.privacy_guard.is_input_blocked() {
                if let Some(pending) = self.pending.take() {
                    pending.cancelled.store(true, Ordering::Release);
                }
                self.busy.store(false, Ordering::Release);
                self.replay.clear();
                self.clear();
                if key.dwExtraInfo != REPLAY_TAG {
                    self.update_key(key.vkCode as usize, down);
                }
                return key.dwExtraInfo == REPLAY_TAG;
            }
            let index = key.vkCode as usize;
            if index < 256 && self.suppressed_up[index] {
                self.suppressed_up[index] = false;
                if !down {
                    self.update_key(index, false);
                    if let Some(pending) = self.pending.as_mut() {
                        pending.keyboard[index] &= 0x7f;
                        pending.trigger_released = true;
                    }
                    return true;
                }
            }
            if self.pending.is_some()
                || (key.dwExtraInfo != REPLAY_TAG
                    && (!self.replay.is_empty() || self.replay_depth > 0))
            {
                let flags = if down {
                    KEYBD_EVENT_FLAGS(0)
                } else {
                    KEYEVENTF_KEYUP
                } | if key.flags.0 & 1 != 0 {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                };
                let input = key_input(
                    VIRTUAL_KEY(key.vkCode as u16),
                    key.scanCode as u16,
                    flags,
                    REPLAY_TAG,
                );
                if let Some(pending) = &mut self.pending {
                    pending.keys.push(input);
                } else {
                    self.replay.push(input);
                }
                return true;
            }
            self.update_key(key.vkCode as usize, down);
            if !down {
                return false;
            }
            let vk = VIRTUAL_KEY(key.vkCode as u16);
            if MODIFIERS.contains(&vk) || [VK_SHIFT, VK_CONTROL, VK_MENU, VK_CAPITAL].contains(&vk)
            {
                return false;
            }
            let target = self.target;
            let mut process_id = 0;
            unsafe {
                GetWindowThreadProcessId(HWND(target.0 as *mut _), Some(&mut process_id));
            }
            if !self.hook.capture_own_process && process_id == std::process::id() {
                self.clear();
                return false;
            }
            if self.keyboard[VK_CONTROL.0 as usize] & 0x80 != 0
                || self.keyboard[VK_MENU.0 as usize] & 0x80 != 0
                || self.keyboard[VK_LWIN.0 as usize] & 0x80 != 0
                || self.keyboard[VK_RWIN.0 as usize] & 0x80 != 0
            {
                self.clear();
                return false;
            }
            if vk == VK_BACK {
                if let Ok(mut b) = self.hook.buffer.try_lock() {
                    b.pop_back();
                }
                return false;
            }
            if [
                VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_HOME, VK_END, VK_PRIOR, VK_NEXT,
                VK_INSERT, VK_DELETE, VK_HANGUL,
            ]
            .contains(&vk)
            {
                self.clear();
                return false;
            }
            // Track physical two-set keys in Korean mode. The worker commits the
            // composition and renders the delivered prefix before erase accounting.
            let layout = unsafe {
                GetKeyboardLayout(GetWindowThreadProcessId(HWND(target.0 as *mut _), None))
            };
            let korean_layout = (layout.0 as usize & 0x3ff) == 0x12;
            let focus = HWND(target.1 as *mut _);
            unsafe {
                let imc = ImmGetContext(focus);
                if !imc.0.is_null() {
                    let composing = ImmGetCompositionStringW(imc, GCS_COMPSTR, None, 0) > 0;
                    let native = ImmGetOpenStatus(imc).as_bool();
                    let _ = ImmReleaseContext(focus, imc);
                    if !korean_layout && (composing || native) {
                        self.clear();
                        return false;
                    }
                }
            }
            let layout = unsafe {
                GetKeyboardLayout(GetWindowThreadProcessId(HWND(target.0 as *mut _), None))
            };
            let mut out = [0u16; 8];
            let count = unsafe {
                ToUnicodeEx(
                    key.vkCode,
                    key.scanCode,
                    &self.keyboard,
                    &mut out,
                    4,
                    Some(layout),
                )
            };
            if count <= 0 {
                if count < 0 {
                    self.clear();
                }
                return false;
            }
            let chars = String::from_utf16_lossy(&out[..(count as usize).min(out.len())]);
            // Suppression/erase accounting is exact only for a single output character.
            if chars.chars().count() != 1 {
                self.clear();
                return false;
            }
            let ch = chars.chars().next().unwrap();
            let ch = if ch == '\r' { '\n' } else { ch };
            if ch.is_control() && ch != '\n' && ch != '\t' {
                self.clear();
                return false;
            }
            let Ok(mut buffer) = self.hook.buffer.try_lock() else {
                return false;
            };
            buffer.push(ch);
            let content = Zeroizing::new(buffer.as_string());
            let Some(matcher) = self.hook.matcher.as_ref() else {
                return false;
            };
            let Ok(matcher) = matcher.try_lock() else {
                buffer.clear();
                return false;
            };
            let snapshot = matcher.clone();
            drop(matcher);
            let profile = self
                .hook
                .profiles
                .as_ref()
                .map_or("common", |p| p.for_process(process_id));
            let Some(matched) = snapshot.find_match_in_profile(&content, profile) else {
                return false;
            };
            buffer.clear();
            if matched.snippet.template
                && matches!(&matched.snippet.content, SnippetContent::PlainText(text) if text.contains("{{") && crate::template::fields(text).is_ok_and(|fields| !fields.is_empty()))
            {
                if let Some(events) = &self.hook.event_sender {
                    let expected = content
                        .chars()
                        .rev()
                        .take(matched.backspace_count)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    let _ = events.send(EngineEvent::InteractiveExpansion {
                        snippet_id: matched.snippet.id,
                        target,
                        expected,
                        revision: self.hook.metrics.input_revision.load(Ordering::Acquire),
                    });
                }
                return false;
            }
            let (text, vault_generation) = match matched.snippet.content {
                SnippetContent::PlainText(text) => (Zeroizing::new(text), None),
                SnippetContent::BitwardenRef { item_id, field } => {
                    let Some(vault) = self.hook.bitwarden.as_ref() else {
                        return false;
                    };
                    let generation = vault.generation();
                    let Ok(value) = vault.get_field(&item_id, &field) else {
                        return false;
                    };
                    (value.into_secret(), Some(generation))
                }
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            self.next_id = self.next_id % 65535 + 1;
            let task = Task {
                id: self.next_id,
                text,
                vault_generation,
                erase: matched.backspace_count.saturating_sub(1),
                modifiers: self.modifiers(),
                target,
                keyword: matched.snippet.keyword,
                expected: content
                    .chars()
                    .rev()
                    .skip(1)
                    .take(matched.backspace_count.saturating_sub(1))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect(),
                password: self.hook.privacy_guard.is_password_field_active(),
                cancelled: Arc::clone(&cancelled),
            };
            let pending = Pending {
                id: self.next_id,
                keyboard: self.keyboard,
                trigger: key_input(vk, key.scanCode as u16, KEYBD_EVENT_FLAGS(0), INPUT_TAG),
                trigger_released: false,
                keys: Vec::with_capacity(64),
                target,
                started: Instant::now(),
                cancelled,
            };
            // Set the gate before dispatch. No second match can overtake this one.
            self.pending = Some(pending);
            self.busy.store(true, Ordering::Release);
            if self.tasks.try_send(task).is_err() {
                self.pending = None;
                self.busy.store(false, Ordering::Release);
                return false;
            }
            self.suppressed_up[index] = true;
            true // Final character/delimiter is withheld, so erase exactly length - 1.
        }
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let key = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let started = Instant::now();
        let intercepted = CONTEXT.with(|slot| {
            let Ok(mut slot) = slot.try_borrow_mut() else {
                return false;
            };
            let Some(ctx) = slot.as_mut() else {
                return false;
            };
            if key.dwExtraInfo == REPLAY_DONE_TAG {
                ctx.replay_depth = ctx.replay_depth.saturating_sub(1);
                if ctx.pending.is_none() && !ctx.replay.is_empty() {
                    let _ = PostThreadMessageW(GetCurrentThreadId(), REPLAY, WPARAM(0), LPARAM(0));
                }
                return true;
            }
            if key.dwExtraInfo == END_TAG {
                if ctx
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == key.scanCode as usize)
                {
                    ctx.finish(false);
                }
                return true;
            }
            if key.dwExtraInfo == INPUT_TAG {
                let moved = ctx
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.target != input_target());
                let protected = ctx.hook.privacy_guard.is_input_blocked();
                return moved
                    || (protected && !MODIFIERS.contains(&VIRTUAL_KEY(key.vkCode as u16)));
            }
            if key.flags.0 & 0x10 != 0
                && key.dwExtraInfo != REPLAY_TAG
                && !(cfg!(debug_assertions) && key.dwExtraInfo == TEST_TAG)
            {
                // Other expanders, accessibility tools and paste operations invalidate our prefix.
                ctx.clear();
                return false;
            }
            let result = ctx.process(
                key,
                wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN,
            );
            ctx.hook.metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            ctx.hook
                .metrics
                .max_callback_us
                .fetch_max(started.elapsed().as_micros() as u64, Ordering::Relaxed);
            result
        });
        if intercepted {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, wparam, lparam)
        }
    }
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0
            && [
                WM_LBUTTONDOWN,
                WM_RBUTTONDOWN,
                WM_MBUTTONDOWN,
                WM_MOUSEWHEEL,
            ]
            .contains(&(wparam.0 as u32))
        {
            CONTEXT.with(|slot| {
                if let Ok(slot) = slot.try_borrow() {
                    if let Some(ctx) = slot.as_ref() {
                        ctx.hook
                            .metrics
                            .input_revision
                            .fetch_add(1, Ordering::AcqRel);
                        ctx.clear();
                        if let Some(pending) = &ctx.pending {
                            pending.cancelled.store(true, Ordering::Release);
                        }
                    }
                }
            });
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
    unsafe extern "system" fn focus_proc(
        _: HWINEVENTHOOK,
        _: u32,
        _: HWND,
        _: i32,
        _: i32,
        _: u32,
        _: u32,
    ) {
        CONTEXT.with(|slot| {
            if let Ok(slot) = slot.try_borrow() {
                if let Some(ctx) = slot.as_ref() {
                    ctx.clear();
                    ctx.hook.privacy_guard.invalidate();
                    if let Some(pending) = &ctx.pending {
                        pending.cancelled.store(true, Ordering::Release);
                    }
                }
            }
        });
    }
    pub(super) fn run(
        hook: KeyboardHook,
        paused: Arc<AtomicBool>,
        alive: Arc<AtomicBool>,
        ready: SyncSender<anyhow::Result<u32>>,
    ) {
        unsafe {
            let thread_id = GetCurrentThreadId();
            let mut msg = MSG::default();
            // Ensure PostThreadMessage can target us before publishing readiness.
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let (tasks, receiver) = sync_channel::<Task>(1);
            let busy = Arc::new(AtomicBool::new(false));
            let spool_busy = Arc::clone(&busy);
            let spool_alive = Arc::clone(&alive);
            let (spool, replay_receiver) =
                std::sync::mpsc::channel::<((usize, usize), Vec<INPUT>)>();
            let replay_worker = std::thread::spawn(move || {
                while let Ok((target, keys)) = replay_receiver.recv() {
                    for input in keys {
                        while spool_busy.load(Ordering::Acquire)
                            && spool_alive.load(Ordering::Acquire)
                        {
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        if !spool_alive.load(Ordering::Acquire) || input_target() != target {
                            break;
                        }
                        if send_packet(&[input]).is_err() {
                            break;
                        }
                        // Preserve a real key transition interval instead of compressing
                        // the physical tail into a zero-time TSF burst.
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    let _ = send_packet(&[key_input(VK_F24, 0, KEYEVENTF_KEYUP, REPLAY_DONE_TAG)]);
                }
            });
            let privacy = Arc::clone(&hook.privacy_guard);
            let events = hook.event_sender.clone();
            let metrics = Arc::clone(&hook.metrics);
            let vault = hook.bitwarden.clone();
            let worker = std::thread::spawn(move || {
                let Ok(mut session) = InjectionSession::new() else {
                    return;
                };
                metrics
                    .worker_window
                    .store(session.owner.0 as usize, Ordering::Release);
                loop {
                    pump_clipboard_messages();
                    match receiver
                        .recv_timeout(session.clipboard_wait().min(Duration::from_millis(5)))
                    {
                        Ok(task) => {
                            let result = if task.cancelled.load(Ordering::Acquire)
                                || privacy.is_input_blocked()
                                || task.vault_generation.is_some_and(|generation| {
                                    !vault.as_ref().is_some_and(|v| v.is_current(generation))
                                }) {
                                Err(anyhow::anyhow!("Substitution cancelled after focus change"))
                            } else {
                                session.substitute(
                                    &task.text,
                                    task.erase,
                                    &task.expected,
                                    &task.modifiers,
                                    task.target,
                                    task.id,
                                    &task.cancelled,
                                    task.password,
                                )
                            };
                            if let Err(error) = result {
                                metrics.recent_error_kind.store(
                                    super::substitution_error_kind(&error.to_string()),
                                    Ordering::Release,
                                );
                                metrics.recent_error.store(true, Ordering::Release);
                                metrics.errors.fetch_add(1, Ordering::Relaxed);
                                tracing::warn!("Substitution failed: {error}");
                                let _ = PostThreadMessageW(
                                    thread_id,
                                    FAILED,
                                    WPARAM(task.id),
                                    LPARAM(0),
                                );
                            } else {
                                metrics.recent_error.store(false, Ordering::Release);
                                metrics.substitutions.fetch_add(1, Ordering::Relaxed);
                                if let Some(ref events) = events {
                                    let _ = events.send(EngineEvent::KeywordMatched(task.keyword));
                                }
                            }
                            if let Some(events) = &events {
                                let _ = events.send(EngineEvent::StatusChanged);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if session.clipboard_wait().is_zero() {
                                session.restore_clipboard();
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
            let mut keyboard = [0u8; 256];
            for &vk in &MODIFIERS {
                if GetAsyncKeyState(vk.0 as i32) < 0 {
                    keyboard[vk.0 as usize] = 0x80;
                }
            }
            keyboard[VK_CAPITAL.0 as usize] = (GetKeyState(VK_CAPITAL.0 as i32) & 1) as u8;
            CONTEXT.with(|slot| {
                *slot.borrow_mut() = Some(Context {
                    hook,
                    paused,
                    keyboard,
                    target: input_target(),
                    pending: None,
                    tasks,
                    next_id: 0,
                    replay: Vec::new(),
                    suppressed_up: [false; 256],
                    busy,
                    replay_depth: 0,
                    spool,
                })
            });
            let keyboard_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0);
            let Ok(keyboard_hook) = keyboard_hook else {
                let _ = ready.send(Err(anyhow::anyhow!("Keyboard hook could not be installed")));
                CONTEXT.with(|s| s.borrow_mut().take());
                return;
            };
            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
            let focus_hook = SetWinEventHook(
                EVENT_OBJECT_FOCUS,
                EVENT_OBJECT_FOCUS,
                None,
                Some(focus_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            let foreground_hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(focus_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            let timer = SetTimer(None, 0, 50, None);
            alive.store(true, Ordering::Release);
            let _ = ready.send(Ok(thread_id));
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                if msg.message == FAILED {
                    CONTEXT.with(|s| {
                        if let Some(ctx) = s.borrow_mut().as_mut() {
                            if ctx
                                .pending
                                .as_ref()
                                .is_some_and(|pending| pending.id == msg.wParam.0)
                            {
                                ctx.finish(true);
                            }
                        }
                    });
                } else if msg.message == REPLAY {
                    CONTEXT.with(|s| {
                        if let Some(ctx) = s.borrow_mut().as_mut() {
                            if ctx.pending.is_none() && !ctx.replay.is_empty() {
                                let keys = std::mem::take(&mut ctx.replay);
                                ctx.replay_depth += 1;
                                if ctx.spool.send((ctx.target, keys)).is_err() {
                                    ctx.replay_depth = ctx.replay_depth.saturating_sub(1);
                                    tracing::warn!("Input replay worker stopped");
                                }
                            }
                        }
                    });
                } else if msg.message == WM_TIMER {
                    CONTEXT.with(|s| {
                        if let Some(ctx) = s.borrow_mut().as_mut() {
                            if let Some(pending) = &ctx.pending {
                                if pending.started.elapsed() > Duration::from_millis(300) {
                                    ctx.hook
                                        .metrics
                                        .recent_error_kind
                                        .store(6, Ordering::Release);
                                    ctx.hook.metrics.recent_error.store(true, Ordering::Release);
                                    pending.cancelled.store(true, Ordering::Release);
                                    // The worker does no unbounded provider calls. A missing barrier means
                                    // injection was rejected or the worker exited; release the gate.
                                    ctx.finish(true);
                                }
                            }
                            if let Ok(mut buffer) = ctx.hook.buffer.try_lock() {
                                buffer.check_timeout_and_clear();
                            }
                        }
                    });
                } else {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            alive.store(false, Ordering::Release);
            let _ = KillTimer(None, timer);
            let _ = UnhookWindowsHookEx(keyboard_hook);
            if let Ok(mouse_hook) = mouse_hook {
                let _ = UnhookWindowsHookEx(mouse_hook);
            }
            let _ = UnhookWinEvent(focus_hook);
            let _ = UnhookWinEvent(foreground_hook);
            CONTEXT.with(|s| {
                if let Some(mut ctx) = s.borrow_mut().take() {
                    ctx.finish(true);
                }
            });
            // A foreign clipboard delayed renderer can stall GetClipboardData.
            // Cancelled work cannot inject after shutdown; never wait on that owner.
            if worker.is_finished() {
                let _ = worker.join();
            }
            if replay_worker.is_finished() {
                let _ = replay_worker.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hook_starts_pauses_and_shuts_down() {
        let mut handle = KeyboardHook::new()
            .with_privacy_guard(Arc::new(crate::MockPrivacyGuard::new(true)))
            .start()
            .unwrap();
        assert!(handle.is_active());
        // Clipboard notifications are synchronous window messages. A thread
        // blocked only on recv() would make another app's EmptyClipboard hang.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while handle.metrics.worker_window.load(Ordering::Acquire) == 0
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let owner = handle.metrics.worker_window.load(Ordering::Acquire);
        assert_ne!(owner, 0);
        unsafe {
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{
                SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_NULL,
            };
            assert_ne!(
                SendMessageTimeoutW(
                    HWND(owner as *mut _),
                    WM_NULL,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    100,
                    None
                )
                .0,
                0,
                "Clipboard owner must acknowledge messages while idle"
            );
        }
        handle.set_paused(true);
        assert!(handle.is_paused());
        handle.stop();
        assert!(!handle.is_active());
    }
}
