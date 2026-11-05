//! Ordered Win32 input packets. Clipboard work runs only on the substitution worker.
use anyhow::{bail, Result};

#[derive(Debug, Default, Clone)]
pub struct TextInjector;
impl TextInjector {
    pub fn new() -> Self {
        Self
    }
    pub fn perform_substitution(&self, text: &str, backspace_count: usize) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let mut session = windows_impl::InjectionSession::new()?;
            session.substitute(
                text,
                backspace_count,
                "",
                &[],
                crate::privacy_guard::input_target(),
                0,
                &std::sync::atomic::AtomicBool::new(false),
                false,
            )?;
            let wait = session.clipboard_wait();
            if wait <= std::time::Duration::from_secs(1) {
                std::thread::sleep(wait);
            }
            session.restore_clipboard();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (text, backspace_count);
            bail!("Windows input is required");
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub(crate) use windows_impl::*;

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use std::mem::size_of;
    use std::time::{Duration, Instant};
    use windows::core::w;
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;
    use windows::Win32::UI::Input::Ime::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use zeroize::Zeroizing;

    pub(crate) const INPUT_TAG: usize = 0x43484C41;
    pub(crate) const END_TAG: usize = 0x43484C42;
    pub(crate) const REPLAY_TAG: usize = 0x43484C43;
    pub(crate) const REPLAY_DONE_TAG: usize = 0x43484C44;
    pub(crate) const MODIFIERS: [VIRTUAL_KEY; 8] = [
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LWIN,
        VK_RWIN,
    ];

    pub(crate) fn pump_clipboard_messages() {
        // Clipboard ownership sends synchronous WM_DESTROYCLIPBOARD/WM_RENDER*
        // messages. An owner thread must pump even while waiting for more work.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    pub(crate) fn key_input(
        vk: VIRTUAL_KEY,
        scan: u16,
        mut flags: KEYBD_EVENT_FLAGS,
        tag: usize,
    ) -> INPUT {
        // Match Beeftext/XMiLib: IME key processing also needs real scan codes.
        // A zero scan code can collapse repeated Backspace/letter events in TSF.
        let scan = if scan == 0 && vk.0 != 0 {
            let mapped = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC_EX) };
            if mapped & 0xff00 != 0 {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            (mapped & 0xff) as u16
        } else {
            scan
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: tag,
                },
            },
        }
    }
    fn key_pair(packet: &mut Vec<INPUT>, vk: VIRTUAL_KEY) {
        packet.push(key_input(vk, 0, KEYBD_EVENT_FLAGS(0), INPUT_TAG));
        packet.push(key_input(vk, 0, KEYEVENTF_KEYUP, INPUT_TAG));
    }
    pub(crate) fn send_packet(packet: &[INPUT]) -> Result<()> {
        if packet.is_empty() {
            return Ok(());
        }
        let sent = unsafe { SendInput(packet, size_of::<INPUT>() as i32) } as usize;
        if sent != packet.len() {
            bail!(
                "SendInput accepted {sent}/{} events. Check target integrity level.",
                packet.len()
            );
        }
        Ok(())
    }
    fn unicode_packet(packet: &mut Vec<INPUT>, text: &str) {
        let normalized = Zeroizing::new(text.replace("\r\n", "\n").replace('\r', "\n"));
        for unit in normalized.encode_utf16() {
            match unit {
                10 => key_pair(packet, VK_RETURN),
                9 => key_pair(packet, VK_TAB),
                _ => {
                    packet.push(key_input(
                        VIRTUAL_KEY(0),
                        unit,
                        KEYEVENTF_UNICODE,
                        INPUT_TAG,
                    ));
                    packet.push(key_input(
                        VIRTUAL_KEY(0),
                        unit,
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        INPUT_TAG,
                    ));
                }
            }
        }
    }
    pub(crate) fn replacement_packet(
        text: &str,
        erase: usize,
        modifiers: &[VIRTUAL_KEY],
        paste: bool,
        barrier: bool,
    ) -> Vec<INPUT> {
        let mut packet = Vec::with_capacity(erase * 2 + text.len() * 2 + 24);
        // Side-specific modifiers only. Generic VK_SHIFT/VK_CONTROL aliases duplicate state.
        for &vk in modifiers {
            packet.push(key_input(vk, 0, KEYEVENTF_KEYUP, INPUT_TAG));
        }
        for _ in 0..erase {
            key_pair(&mut packet, VK_BACK);
        }
        if paste {
            packet.push(key_input(VK_LCONTROL, 0, KEYBD_EVENT_FLAGS(0), INPUT_TAG));
            key_pair(&mut packet, VIRTUAL_KEY(0x56));
            packet.push(key_input(VK_LCONTROL, 0, KEYEVENTF_KEYUP, INPUT_TAG));
        } else {
            unicode_packet(&mut packet, text);
        }
        for &vk in modifiers {
            packet.push(key_input(vk, 0, KEYBD_EVENT_FLAGS(0), INPUT_TAG));
        }
        // Consumed by our hook. Completion is observed after all preceding input events.
        if barrier {
            packet.push(key_input(VK_F24, 0, KEYEVENTF_KEYUP, END_TAG));
        }
        packet
    }

    struct ClipboardOpen;
    impl ClipboardOpen {
        fn open(owner: HWND) -> Result<Self> {
            let deadline = Instant::now() + Duration::from_millis(60);
            loop {
                pump_clipboard_messages();
                if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
                    return Ok(Self);
                }
                if Instant::now() >= deadline {
                    bail!("Clipboard is busy");
                }
                std::thread::sleep(Duration::from_millis(3));
            }
        }
    }
    impl Drop for ClipboardOpen {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    // Only independently copyable global-memory formats are admitted. If a bitmap,
    // metafile or delayed renderer cannot be preserved, use Unicode without touching it.
    struct ClipboardSnapshot {
        formats: Vec<(u32, Zeroizing<Vec<u8>>)>,
        sequence: u32,
        deadline: Instant,
        text: Zeroizing<Vec<u16>>,
    }
    unsafe fn snapshot() -> Result<Vec<(u32, Zeroizing<Vec<u8>>)>> {
        let mut formats = Vec::new();
        let mut format = EnumClipboardFormats(0);
        let mut total = 0;
        while format != 0 {
            if [2, 3, 9, 14, 0x80, 0x81, 0x82, 0x83].contains(&format) {
                bail!("Non-memory clipboard format");
            }
            let handle = GetClipboardData(format)?;
            let size = GlobalSize(HGLOBAL(handle.0));
            total += size;
            if size == 0 || total > 16 * 1024 * 1024 || formats.len() >= 128 {
                bail!("Clipboard snapshot exceeds safe bounds");
            }
            let ptr = GlobalLock(HGLOBAL(handle.0));
            if ptr.is_null() {
                bail!("Clipboard data cannot be locked");
            }
            let bytes = Zeroizing::new(std::slice::from_raw_parts(ptr as *const u8, size).to_vec());
            let _ = GlobalUnlock(HGLOBAL(handle.0));
            formats.push((format, bytes));
            format = EnumClipboardFormats(format);
        }
        Ok(formats)
    }
    unsafe fn put(format: u32, bytes: &[u8]) -> Result<()> {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let ptr = GlobalLock(memory);
        if ptr.is_null() {
            let _ = GlobalFree(Some(memory));
            bail!("Clipboard allocation cannot be locked");
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        let _ = GlobalUnlock(memory);
        if let Err(error) = SetClipboardData(format, Some(HANDLE(memory.0))) {
            let _ = GlobalFree(Some(memory));
            return Err(error.into());
        }
        Ok(())
    }
    fn crlf(text: &str) -> Zeroizing<String> {
        let mut normalized = Zeroizing::new(String::with_capacity(text.len() * 2));
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\r' || ch == '\n' {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                normalized.push_str("\r\n");
            } else {
                normalized.push(ch);
            }
        }
        normalized
    }

    fn message(window: HWND, msg: u32, wparam: usize, lparam: isize) -> Result<usize> {
        let mut result = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                window,
                msg,
                WPARAM(wparam),
                LPARAM(lparam),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                40,
                Some(&mut result),
            )
        };
        if sent.0 == 0 {
            bail!("The text control did not acknowledge an edit message");
        }
        Ok(result)
    }

    /// Native RichEdit messages form one undoable edit and bypass TSF's
    /// asynchronous Unicode key translation. This includes Windows 11 Notepad.
    /// Only the exact observed ASCII keyword at a collapsed caret is replaced.
    fn replace_native(
        text: &str,
        erase: usize,
        keyword: &str,
        target: (usize, usize),
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<bool> {
        if keyword.is_empty() {
            return Ok(false);
        }
        let focus = HWND(target.1 as *mut _);
        let mut class = [0u16; 128];
        let count = unsafe { GetClassNameW(focus, &mut class) };
        let class = String::from_utf16_lossy(&class[..count.max(0) as usize]).to_ascii_lowercase();
        if class != "edit" && !class.starts_with("richedit") {
            return Ok(false);
        }
        if unsafe { GetWindowLongPtrW(focus, GWL_STYLE) } & 0x20 != 0 {
            return Ok(false);
        }
        const EM_GETSEL: u32 = 0x00b0;
        const EM_SETSEL: u32 = 0x00b1;
        const EM_REPLACESEL: u32 = 0x00c2;
        const EM_LINEFROMCHAR: u32 = 0x00c9;
        const EM_LINEINDEX: u32 = 0x00bb;
        const EM_GETLINE: u32 = 0x00c4;
        let expected: Vec<u16> = keyword.encode_utf16().take(erase).collect();
        let deadline = Instant::now() + Duration::from_millis(90);
        loop {
            if cancelled.load(std::sync::atomic::Ordering::Acquire)
                || crate::privacy_guard::input_target() != target
            {
                bail!("Input focus changed before native replacement");
            }
            let (mut start, mut end) = (0u32, 0u32);
            message(
                focus,
                EM_GETSEL,
                &mut start as *mut u32 as usize,
                &mut end as *mut u32 as isize,
            )?;
            if start != end {
                bail!("Text selection changed during expansion");
            }
            let line = message(focus, EM_LINEFROMCHAR, start as usize, 0)?;
            let line_start = message(focus, EM_LINEINDEX, line, 0)?;
            if line_start > start as usize {
                bail!("Invalid native caret position");
            }
            let column = start as usize - line_start;
            if column > 65534 {
                return Ok(false);
            }
            // Read only the caret's current line. Edit and RichEdit use different
            // CR/LF position rules, so a whole-document text prefix is incorrect.
            let mut prefix = Zeroizing::new(vec![0u16; column.max(1) + 1]);
            prefix[0] = column.max(1) as u16;
            let read = message(focus, EM_GETLINE, line, prefix.as_mut_ptr() as isize)?;
            let prefix = &prefix[..read.min(column)];
            if prefix.ends_with(&expected) {
                if cancelled.load(std::sync::atomic::Ordering::Acquire)
                    || crate::privacy_guard::input_target() != target
                {
                    bail!("Input focus changed before native replacement");
                }
                message(
                    focus,
                    EM_SETSEL,
                    start as usize - expected.len(),
                    end as isize,
                )?;
                let replacement: Zeroizing<Vec<u16>> =
                    Zeroizing::new(crlf(text).encode_utf16().chain(Some(0)).collect());
                message(focus, EM_REPLACESEL, 1, replacement.as_ptr() as isize)?;
                return Ok(true);
            }
            if Instant::now() >= deadline {
                bail!("Typed keyword was not acknowledged by the native text control");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    trait ImeControl {
        fn mode(&self) -> Result<u32>;
        fn set_mode(&self, mode: u32) -> Result<()>;
    }
    struct WindowIme(HWND);
    impl ImeControl for WindowIme {
        fn mode(&self) -> Result<u32> {
            let mut mode = 0usize;
            let ok = unsafe {
                SendMessageTimeoutW(
                    self.0,
                    WM_IME_CONTROL,
                    WPARAM(1),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    40,
                    Some(&mut mode),
                )
                .0 != 0
            };
            if !ok {
                bail!("Korean input mode could not be verified");
            }
            Ok(mode as u32)
        }
        fn set_mode(&self, mode: u32) -> Result<()> {
            let ok = unsafe {
                SendMessageTimeoutW(
                    self.0,
                    WM_IME_CONTROL,
                    WPARAM(IMC_SETCONVERSIONMODE as usize),
                    LPARAM(mode as isize),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    40,
                    None,
                )
                .0 != 0
            };
            if !ok {
                bail!("Korean input mode could not be changed");
            }
            Ok(())
        }
    }
    // A foreign HIMC is unavailable in TSF apps. Ask the owning IME window to
    // commit Korean composition by temporarily selecting alphanumeric mode.
    // Restore the full original mode before releasing the queued physical tail.
    struct KoreanInput<C: ImeControl> {
        control: C,
        original: Option<u32>,
        rendered: Option<String>,
    }
    impl<C: ImeControl> KoreanInput<C> {
        fn begin(control: C, expected: &str) -> Result<Self> {
            let mode = control.mode()?;
            let native = mode & IME_CMODE_NATIVE.0 != 0;
            let guard = Self {
                control,
                original: native.then_some(mode),
                rendered: native.then(|| crate::korean::render(expected)),
            };
            if native {
                guard.control.set_mode(mode & !IME_CMODE_NATIVE.0)?;
                if guard.control.mode()? & IME_CMODE_NATIVE.0 != 0 {
                    bail!("Korean composition could not be completed");
                }
            }
            Ok(guard)
        }
        fn restore(&mut self) -> Result<()> {
            if let Some(mode) = self.original {
                self.control.set_mode(mode)?;
                if self.control.mode()? != mode {
                    bail!("Korean input mode could not be restored");
                }
                self.original = None;
            }
            Ok(())
        }
    }
    impl<C: ImeControl> Drop for KoreanInput<C> {
        fn drop(&mut self) {
            if self.restore().is_err() {
                tracing::warn!("Korean input mode restoration failed");
            }
        }
    }
    fn korean_input(
        expected: &str,
        target: (usize, usize),
    ) -> Result<Option<KoreanInput<WindowIme>>> {
        if expected.is_empty() || !expected.is_ascii() {
            return Ok(None);
        }
        unsafe {
            let thread = GetWindowThreadProcessId(HWND(target.0 as *mut _), None);
            if GetKeyboardLayout(thread).0 as usize & 0x3ff != 0x12 {
                return Ok(None);
            }
            let ime = ImmGetDefaultIMEWnd(HWND(target.1 as *mut _));
            if ime.is_invalid() {
                bail!("Korean input mode could not be verified");
            }
            let guard = KoreanInput::begin(WindowIme(ime), expected)?;
            if guard.original.is_some() {
                std::thread::sleep(Duration::from_millis(8));
            }
            Ok(Some(guard))
        }
    }
    fn restore_ime(guard: &mut Option<KoreanInput<WindowIme>>) {
        if let Some(guard) = guard {
            // Text is already inserted. A restoration error must never replay
            // the withheld key into the completed replacement.
            if guard.restore().is_err() {
                tracing::warn!("Korean input mode restoration failed");
            }
        }
    }

    pub(crate) struct InjectionSession {
        pub(crate) owner: HWND,
        clipboard: Option<ClipboardSnapshot>,
    }
    impl InjectionSession {
        pub(crate) fn new() -> Result<Self> {
            let owner = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("Challa clipboard"),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )?
            };
            Ok(Self {
                owner,
                clipboard: None,
            })
        }
        pub(crate) fn clipboard_wait(&self) -> Duration {
            self.clipboard
                .as_ref()
                .map(|c| c.deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(60))
        }
        fn prepare_clipboard(&mut self, text: &str) -> Result<()> {
            self.restore_clipboard();
            // If restoration is temporarily blocked, keep the existing backup and use Unicode.
            if self.clipboard.is_some() {
                bail!("Prior clipboard restoration pending");
            }
            let open = ClipboardOpen::open(self.owner)?;
            unsafe {
                let formats = snapshot()?;
                EmptyClipboard()?;
                let wide: Zeroizing<Vec<u16>> =
                    Zeroizing::new(crlf(text).encode_utf16().chain(Some(0)).collect());
                let result = (|| -> Result<()> {
                    for name in [
                        w!("CanIncludeInClipboardHistory"),
                        w!("CanUploadToCloudClipboard"),
                        w!("ExcludeClipboardContentFromMonitorProcessing"),
                    ] {
                        let format = RegisterClipboardFormatW(name);
                        if format == 0 {
                            bail!("Clipboard privacy format unavailable");
                        }
                        put(format, &0u32.to_ne_bytes())?;
                    }
                    let bytes =
                        std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                    put(13, bytes)
                })();
                if result.is_err() {
                    let _ = EmptyClipboard();
                    for (format, bytes) in &formats {
                        put(*format, bytes)?;
                    }
                    return result;
                }
                let sequence = GetClipboardSequenceNumber();
                drop(open);
                self.clipboard = Some(ClipboardSnapshot {
                    formats,
                    sequence,
                    deadline: Instant::now() + Duration::from_secs(1),
                    text: wide,
                });
            }
            Ok(())
        }
        pub(crate) fn restore_clipboard(&mut self) {
            let Some(snapshot) = self.clipboard.as_mut() else {
                return;
            };
            // Closing a clipboard and Windows' synthesized text formats may advance
            // the sequence. Ownership + exact payload distinguishes those changes
            // from a user's newer copy operation.
            if unsafe { GetClipboardOwner() }.ok() != Some(self.owner) {
                self.clipboard = None;
                return;
            }
            let Ok(_open) = ClipboardOpen::open(self.owner) else {
                snapshot.deadline = Instant::now() + Duration::from_millis(100);
                return;
            };
            if unsafe { GetClipboardOwner() }.ok() != Some(self.owner) {
                self.clipboard = None;
                return;
            }
            if unsafe { GetClipboardSequenceNumber() } != snapshot.sequence {
                let still_ours = unsafe {
                    GetClipboardData(13).ok().is_some_and(|handle| {
                        let memory = HGLOBAL(handle.0);
                        let size = GlobalSize(memory);
                        if size < snapshot.text.len() * 2 || size > 1024 * 1024 {
                            return false;
                        }
                        let ptr = GlobalLock(memory);
                        if ptr.is_null() {
                            return false;
                        }
                        let matches =
                            std::slice::from_raw_parts(ptr as *const u16, snapshot.text.len())
                                == snapshot.text.as_slice();
                        let _ = GlobalUnlock(memory);
                        matches
                    })
                };
                if !still_ours {
                    self.clipboard = None;
                    return;
                }
            }
            unsafe {
                if EmptyClipboard().is_err() {
                    return;
                }
                for (format, bytes) in &snapshot.formats {
                    if let Err(error) = put(*format, bytes) {
                        tracing::error!("Clipboard restoration failed: {error}");
                    }
                }
            }
            self.clipboard = None;
        }
        pub(crate) fn substitute(
            &mut self,
            text: &str,
            erase: usize,
            keyword: &str,
            modifiers: &[VIRTUAL_KEY],
            target: (usize, usize),
            id: usize,
            cancelled: &std::sync::atomic::AtomicBool,
            password: bool,
        ) -> Result<()> {
            if erase > 256 || text.encode_utf16().count() > 16_384 {
                bail!("Substitution exceeds input limit");
            }
            // Earlier physical keys must arrive before completing an IME composition.
            std::thread::sleep(Duration::from_millis(12));
            if cancelled.load(std::sync::atomic::Ordering::Acquire)
                || target == (0, 0)
                || target != crate::privacy_guard::input_target()
            {
                bail!("Input focus changed before substitution");
            }
            let mut ime = korean_input(keyword, target)?;
            let rendered = ime.as_ref().and_then(|g| g.rendered.as_ref());
            let (keyword, erase) =
                rendered.map_or((keyword, erase), |s| (s.as_str(), s.chars().count()));
            if !password
                && !keyword.contains(['\r', '\n'])
                && replace_native(text, erase, keyword, target, cancelled)?
            {
                restore_ime(&mut ime);
                if id != 0 {
                    send_packet(&[key_input(VK_F24, id as u16, KEYEVENTF_KEYUP, END_TAG)])?;
                }
                return Ok(());
            }
            // One paste operation avoids asynchronous WM_CHAR interleaving in
            // controls without the native edit contract, as in Beeftext.
            // Password destinations receive Unicode input without clipboard exposure.
            let paste = !password && self.prepare_clipboard(text).is_ok();
            if !paste && !password && text.encode_utf16().count() > 96 {
                bail!("Clipboard could not be preserved; expansion was cancelled before erasing the keyword");
            }
            // The LL callback precedes delivery to the target. Let its earlier
            // WM_KEYDOWN/WM_CHAR messages settle while the input gate holds the tail.
            // This delay is per expansion, never per normal keystroke or on the hook.
            if cancelled.load(std::sync::atomic::Ordering::Acquire)
                || target == (0, 0)
                || target != crate::privacy_guard::input_target()
            {
                self.restore_clipboard();
                bail!("Input focus changed before substitution");
            }
            let mut packet = replacement_packet(text, erase, modifiers, paste, false);
            if !paste && !password {
                // A short Unicode fallback is paced on this worker, never on
                // the hook. The input gate stays closed until it finishes.
                for index in 0..packet.len() {
                    if cancelled.load(std::sync::atomic::Ordering::Acquire)
                        || target != crate::privacy_guard::input_target()
                    {
                        packet.fill(INPUT::default());
                        bail!("Input focus changed during substitution");
                    }
                    if let Err(error) = send_packet(&[packet[index]]) {
                        packet.fill(INPUT::default());
                        return Err(error);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                packet.fill(INPUT::default());
                packet.clear();
            }
            let result = send_packet(&packet);
            packet.fill(INPUT::default());
            if ime.as_ref().is_some_and(|g| g.original.is_some()) && result.is_ok() {
                std::thread::sleep(Duration::from_millis(12));
            }
            restore_ime(&mut ime);
            if result.is_err() {
                self.restore_clipboard();
            }
            result?;
            if id != 0 {
                send_packet(&[key_input(VK_F24, id as u16, KEYEVENTF_KEYUP, END_TAG)])?;
            }
            Ok(())
        }
    }
    impl Drop for InjectionSession {
        fn drop(&mut self) {
            self.restore_clipboard();
            unsafe {
                let _ = DestroyWindow(self.owner);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[derive(Clone)]
        struct FakeIme {
            mode: std::rc::Rc<std::cell::Cell<u32>>,
            changes: std::rc::Rc<std::cell::RefCell<Vec<u32>>>,
            reject_clear: bool,
        }
        impl FakeIme {
            fn new(mode: u32) -> Self {
                Self {
                    mode: std::rc::Rc::new(std::cell::Cell::new(mode)),
                    changes: Default::default(),
                    reject_clear: false,
                }
            }
        }
        impl ImeControl for FakeIme {
            fn mode(&self) -> Result<u32> {
                Ok(self.mode.get())
            }
            fn set_mode(&self, mode: u32) -> Result<()> {
                self.changes.borrow_mut().push(mode);
                if self.reject_clear && mode & IME_CMODE_NATIVE.0 == 0 {
                    bail!("test transition failure");
                }
                self.mode.set(mode);
                Ok(())
            }
        }
        #[test]
        fn english_mode_needs_no_ime_context_or_mode_change() {
            let ime = FakeIme::new(0);
            let guard = KoreanInput::begin(ime.clone(), "#rks").unwrap();
            assert!(guard.rendered.is_none());
            drop(guard);
            assert!(ime.changes.borrow().is_empty());
        }
        #[test]
        fn korean_mode_commits_and_restores_all_original_flags() {
            let ime = FakeIme::new(9);
            let mut guard = KoreanInput::begin(ime.clone(), "#rks").unwrap();
            assert_eq!(ime.mode.get(), 8);
            assert_eq!(guard.rendered.as_deref(), Some("#간"));
            guard.restore().unwrap();
            drop(guard);
            assert_eq!(ime.mode.get(), 9);
            assert_eq!(*ime.changes.borrow(), vec![8, 9]);
        }
        #[test]
        fn cancelled_expansion_restores_the_korean_mode() {
            let ime = FakeIme::new(9);
            let guard = KoreanInput::begin(ime.clone(), "#rks").unwrap();
            drop(guard);
            assert_eq!(ime.mode.get(), 9);
            assert_eq!(*ime.changes.borrow(), vec![8, 9]);
        }
        #[test]
        fn failed_transition_restores_mode_before_any_text_is_erased() {
            let mut ime = FakeIme::new(9);
            ime.reject_clear = true;
            assert!(KoreanInput::begin(ime.clone(), "#rks").is_err());
            assert_eq!(ime.mode.get(), 9);
            assert_eq!(*ime.changes.borrow(), vec![8, 9]);
        }
        #[test]
        fn password_packet_uses_unicode_without_a_paste_shortcut() {
            let packet = replacement_packet("synthetic", 4, &[VK_LSHIFT], false, true);
            let keys: Vec<_> = packet.iter().map(|i| unsafe { i.Anonymous.ki }).collect();
            assert_eq!(keys.iter().filter(|k| k.wVk == VK_BACK).count(), 8);
            assert!(!keys.iter().any(|k| k.wVk == VK_LCONTROL));
            assert_eq!(
                keys.iter()
                    .filter(|k| k.dwFlags.contains(KEYEVENTF_UNICODE))
                    .count(),
                18
            );
        }
        #[test]
        fn unicode_newlines_are_real_enter_keys_and_surrogates_are_pairs() {
            let packet = replacement_packet("한\n😀", 6, &[VK_LSHIFT], false, true);
            let keys: Vec<_> = packet.iter().map(|i| unsafe { i.Anonymous.ki }).collect();
            assert_eq!(keys.iter().filter(|k| k.wVk == VK_BACK).count(), 12);
            assert!(keys
                .iter()
                .filter(|k| k.wVk == VK_BACK)
                .all(|k| k.wScan != 0));
            assert_eq!(keys.iter().filter(|k| k.wVk == VK_RETURN).count(), 2);
            assert_eq!(
                keys.iter()
                    .filter(|k| k.dwFlags.contains(KEYEVENTF_UNICODE))
                    .count(),
                6
            );
            assert_eq!(keys.last().unwrap().dwExtraInfo, END_TAG);
        }
        #[test]
        fn mixed_line_endings_are_normalized() {
            assert_eq!(crlf("a\r\nb\nc\rd").as_str(), "a\r\nb\r\nc\r\nd");
        }
    }
}
