"""Native regression probe. Run on an unlocked Windows desktop, with no Challa app active.

uv run --with pywin32 python scripts/test_windows_input.py
Build the debug hook first: cargo build -p challa-core --example hook_probe.
Creates only an owned test window. Restores the original clipboard and input locale.
"""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import win32clipboard as cb
import win32con as con
import win32gui as gui
import win32api
import win32process

ROOT = Path(__file__).resolve().parents[1]
if "--all" in sys.argv:
    for arguments in ([], ["--notepad"], ["--clipboard-path"]):
        completed = subprocess.run([sys.executable, __file__, *arguments], cwd=ROOT, stdin=subprocess.DEVNULL)
        if completed.returncode:
            sys.exit(completed.returncode)
    sys.exit(0)
NOTEPAD = "--notepad" in sys.argv
BASELINE = "--baseline" in sys.argv
CLIPBOARD_PATH = "--clipboard-path" in sys.argv
OUT = ROOT / "docs" / "native-input" / (("notepad" if NOTEPAD else "clipboard" if CLIPBOARD_PATH else "edit") + ("-baseline" if BASELINE else ""))
OUT.mkdir(parents=True, exist_ok=True)
TAG = 0x43485453
user = c.WinDLL("user32", use_last_error=True)
class KEYBDINPUT(c.Structure):
    _fields_ = [("vk", w.WORD), ("scan", w.WORD), ("flags", w.DWORD), ("time", w.DWORD), ("extra", c.c_size_t)]
class DATA(c.Union):
    _fields_ = [("ki", KEYBDINPUT), ("padding", c.c_byte * 32)]
class INPUT(c.Structure):
    _fields_ = [("type", w.DWORD), ("data", DATA)]
class GUIINFO(c.Structure):
    _fields_ = [("cbSize", w.DWORD), ("flags", w.DWORD), ("active", w.HWND), ("focus", w.HWND), ("capture", w.HWND), ("menu", w.HWND), ("move", w.HWND), ("caret", w.HWND), ("rect", w.RECT)]
class WNDCLASS(c.Structure):
    _fields_ = [("style", w.UINT), ("procedure", c.c_void_p), ("class_extra", c.c_int), ("window_extra", c.c_int), ("instance", w.HANDLE), ("icon", w.HANDLE), ("cursor", w.HANDLE), ("background", w.HANDLE), ("menu_name", w.LPCWSTR), ("class_name", w.LPCWSTR)]
user.SendInput.argtypes = [w.UINT, c.POINTER(INPUT), c.c_int]
user.VkKeyScanW.argtypes = [w.WCHAR]
user.VkKeyScanW.restype = c.c_short
user.GetKeyboardLayout.restype = c.c_void_p
user.LoadKeyboardLayoutW.argtypes = [w.LPCWSTR, w.UINT]
user.LoadKeyboardLayoutW.restype = c.c_void_p
user.ActivateKeyboardLayout.argtypes = [c.c_void_p, w.UINT]
user.GetClipboardSequenceNumber.restype = w.DWORD
user.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
user.SendMessageW.restype = c.c_ssize_t
kernel = c.WinDLL("kernel32", use_last_error=True)
kernel.GlobalAlloc.argtypes = [w.UINT, c.c_size_t]
kernel.GlobalAlloc.restype = w.HANDLE
kernel.GlobalLock.argtypes = [w.HANDLE]
kernel.GlobalLock.restype = c.c_void_p
kernel.GlobalSize.argtypes = [w.HANDLE]
kernel.GlobalSize.restype = c.c_size_t
kernel.GlobalUnlock.argtypes = [w.HANDLE]
kernel.GlobalFree.argtypes = [w.HANDLE]
user.SetClipboardData.argtypes = [w.UINT, w.HANDLE]
user.SetClipboardData.restype = w.HANDLE
user.GetClipboardData.argtypes = [w.UINT]
user.GetClipboardData.restype = w.HANDLE
imm = c.WinDLL("imm32")
imm.ImmGetDefaultIMEWnd.argtypes = [w.HWND]
imm.ImmGetDefaultIMEWnd.restype = w.HWND

def key(vk, up=False, extended=False):
    scan = user.MapVirtualKeyW(vk, 4)  # MAPVK_VK_TO_VSC_EX
    return INPUT(1, DATA(ki=KEYBDINPUT(vk, scan & 255, (2 if up else 0) | (1 if extended or scan & 0xff00 else 0), 0, TAG)))

def events(text):
    result = []
    for char in text:
        value = user.VkKeyScanW(char)
        if value == -1:
            raise ValueError(f"Unsupported test key {char!r}")
        vk, mods = value & 255, (value >> 8) & 255
        held = [code for bit, code in [(1, con.VK_LSHIFT), (2, con.VK_LCONTROL), (4, con.VK_LMENU)] if mods & bit]
        result.extend(key(code) for code in held)
        result.extend([key(vk), key(vk, True)])
        result.extend(key(code, True) for code in reversed(held))
    return result

def send(items):
    array = (INPUT * len(items))(*items)
    sent = user.SendInput(len(items), array, c.sizeof(INPUT))
    assert sent == len(items), f"SendInput: {sent}/{len(items)}"

def pump(seconds=0.04):
    until = time.perf_counter() + seconds
    while time.perf_counter() < until:
        gui.PumpWaitingMessages()
        time.sleep(0.001)

def value(edit):
    length = gui.SendMessage(edit, con.WM_GETTEXTLENGTH, 0, 0)
    buffer = c.create_unicode_buffer(length + 1)
    user.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
    user.SendMessageW(edit, con.WM_GETTEXT, length + 1, c.addressof(buffer))
    return buffer.value.replace("\r\n", "\n")

def set_text(edit, text):
    # SetWindowText does not reliably change controls in another process.
    # These documented edit messages are marshalled across process boundaries.
    buffer = c.create_unicode_buffer(text)
    user.SendMessageW(edit, 0x00b1, 0, -1)  # EM_SETSEL
    user.SendMessageW(edit, 0x00c2, 1, c.addressof(buffer))  # EM_REPLACESEL

def restore_original_clipboard(owner, formats):
    cb.OpenClipboard(owner)
    try:
        cb.EmptyClipboard()
        for format, data in formats:
            payload = data.encode("utf-16-le") + b"\0\0" if isinstance(data, str) else data
            if format in (con.CF_TEXT, con.CF_OEMTEXT) and not payload.endswith(b"\0"):
                payload += b"\0"
            memory = kernel.GlobalAlloc(2, max(1, len(payload)))  # GMEM_MOVEABLE
            if not memory:
                raise MemoryError("Clipboard restoration allocation failed")
            pointer = kernel.GlobalLock(memory)
            if not pointer:
                kernel.GlobalFree(memory)
                raise RuntimeError("Clipboard restoration lock failed")
            c.memmove(pointer, payload, len(payload))
            kernel.GlobalUnlock(memory)
            if not user.SetClipboardData(format, memory):
                error = c.get_last_error()
                kernel.GlobalFree(memory)
                raise RuntimeError(f"Clipboard restoration failed for format {format}: {error}")
    finally:
        cb.CloseClipboard()

results = []
def check(name, edit, typed, expected, burst=False, prefix=""):
    assert gui.GetForegroundWindow() == window, f"Foreground changed before {name}; input test stopped"
    info = GUIINFO(); info.cbSize = c.sizeof(info)
    user.GetGUIThreadInfo(win32process.GetWindowThreadProcessId(window)[0], c.byref(info))
    assert info.focus == edit, f"Text focus changed before {name}; input test stopped"
    send([key(con.VK_ESCAPE), key(con.VK_ESCAPE, True)])
    pump(0.08)
    set_text(edit, prefix)
    pump(0.2)
    assert value(edit) == prefix.replace("\r\n", "\n"), f"Could not reset the owned test document before {name}"
    started = time.perf_counter()
    if burst and BASELINE:
        send(events(typed))
    else:
        for char in typed:
            assert gui.GetForegroundWindow() == window, f"Foreground changed during {name}; input test stopped"
            send(events(char))
            # 67 characters/s stresses the gate with physical inter-key time.
            # --baseline separately records zero-time synthetic delivery with
            # Challa paused, without attributing target-side delivery to the engine.
            pump(0.015 if burst else 0.03)
    deadline = time.perf_counter() + 2
    while time.perf_counter() < deadline:
        pump(0.005)
        actual = value(edit)
        if actual == expected:
            break
    actual = value(edit)
    row = {"scenario": name, "passed": actual == expected, "foreground_unchanged": gui.GetForegroundWindow() == window, "elapsed_ms": round((time.perf_counter() - started) * 1000, 2), "actual": actual, "expected": expected}
    results.append(row)
    print(json.dumps({key: value for key, value in row.items() if key not in ("actual", "expected")}, ensure_ascii=False), flush=True)
    assert actual == expected, name

# Deep-copy values. OleGetClipboard returns a live proxy, not a snapshot; setting
# that proxy back can make the clipboard recursively refer to itself.
cb.OpenClipboard()
original = []
try:
    format = cb.EnumClipboardFormats(0)
    while format:
        if format in (2, 3, 9, 14, 0x80, 0x81, 0x82, 0x83):
            original = None
            break
        memory = user.GetClipboardData(format)
        size = kernel.GlobalSize(memory) if memory else 0
        if not size or size > 16 * 1024 * 1024:
            original = None
            break
        pointer = kernel.GlobalLock(memory)
        if not pointer:
            original = None
            break
        data = c.string_at(pointer, size)
        kernel.GlobalUnlock(memory)
        original.append((format, data))
        format = cb.EnumClipboardFormats(format)
finally:
    (OUT / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
    cb.CloseClipboard()
locale = user.GetKeyboardLayout(0)
ready, stop = OUT / "ready", OUT / "stop"
ready.unlink(missing_ok=True)
stop.unlink(missing_ok=True)
stdout = (OUT / "probe.log").open("w", encoding="utf-8")
probe = None
window = None
ime_window = None
ime_open = None
original_target_layout = None
original_ime_conversion = None
document_label = None
try:
    user.LoadKeyboardLayoutW("00000409", 1)
    if NOTEPAD:
        document_label = f"challa-regression-{os.getpid()}"
        document = OUT / f"{document_label}.txt"
        document.write_text("", encoding="utf-8")
        subprocess.Popen(["notepad.exe", str(document)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.perf_counter() + 10
        windows = []
        while not windows and time.perf_counter() < deadline:
            gui.EnumWindows(lambda handle, _: windows.append(handle) if document_label in gui.GetWindowText(handle) else None, None)
            pump(0.05)
        assert len(windows) == 1, "Could not identify the owned Notepad document"
        window = windows[0]
        children = []
        gui.EnumChildWindows(window, lambda handle, _: children.append((handle, gui.GetClassName(handle))), None)
        edits = [handle for handle, name in children if gui.IsWindowVisible(handle) and ("richedit" in name.lower() or name.lower() == "edit")]
        assert edits, f"Owned Notepad text control not found: {children}"
        edit = edits[0]
    else:
        window = gui.CreateWindowEx(0, "STATIC", "Challa native input regression", con.WS_OVERLAPPEDWINDOW | con.WS_VISIBLE, 120, 120, 750, 420, 0, 0, 0, None)
        edit_class = "EDIT"
        edit_instance = 0
        if CLIPBOARD_PATH:
            template = WNDCLASS()
            user.GetClassInfoW.argtypes = [w.HANDLE, w.LPCWSTR, c.POINTER(WNDCLASS)]
            user.RegisterClassW.argtypes = [c.POINTER(WNDCLASS)]
            kernel.GetModuleHandleW.argtypes = [w.LPCWSTR]
            kernel.GetModuleHandleW.restype = w.HANDLE
            assert user.GetClassInfoW(None, "EDIT", c.byref(template))
            edit_class = "ChallaClipboardProbe"
            template.class_name = edit_class
            template.instance = kernel.GetModuleHandleW(None)
            edit_instance = template.instance
            assert user.RegisterClassW(c.byref(template))
        edit = gui.CreateWindowEx(0, edit_class, "", con.WS_CHILD | con.WS_VISIBLE | con.WS_BORDER | con.ES_MULTILINE | con.ES_AUTOVSCROLL, 10, 10, 700, 300, window, 1, edit_instance, None)
    active_thread = win32process.GetWindowThreadProcessId(gui.GetForegroundWindow())[0]
    current_thread = win32api.GetCurrentThreadId()
    user.AttachThreadInput.argtypes = [w.DWORD, w.DWORD, w.BOOL]
    attached = active_thread != current_thread and user.AttachThreadInput(current_thread, active_thread, True)
    try:
        user.SetForegroundWindow.argtypes = [w.HWND]
        user.SetForegroundWindow(window)
        if gui.GetForegroundWindow() != window:
            # Windows grants foreground activation to the source of the last input.
            send([key(con.VK_LMENU), key(con.VK_LMENU, True)])
            user.SetForegroundWindow(window)
        target_thread = win32process.GetWindowThreadProcessId(window)[0]
        target_attached = target_thread != current_thread and target_thread != active_thread and user.AttachThreadInput(current_thread, target_thread, True)
        try:
            gui.SetFocus(edit)
        finally:
            if target_attached:
                user.AttachThreadInput(current_thread, target_thread, False)
    finally:
        if attached:
            user.AttachThreadInput(current_thread, active_thread, False)
    assert gui.GetForegroundWindow() == window, "Owned test window could not take foreground focus"
    if NOTEPAD:
        original_target_layout = user.GetKeyboardLayout(target_thread)
        ime_window = imm.ImmGetDefaultIMEWnd(edit)
        ime_open = gui.SendMessage(ime_window, 0x0283, 0x0005, 0)
        original_ime_conversion = gui.SendMessage(ime_window, 0x0283, 0x0001, 0)
        gui.SendMessage(ime_window, 0x0283, 0x0006, 0)
        pump(0.15)
    command = [str(ROOT / "target/debug/examples/hook_probe.exe"), str(ready), str(stop)]
    if BASELINE:
        command.append("--paused")
    probe = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=stdout, stderr=subprocess.STDOUT)
    deadline = time.perf_counter() + 10
    while not ready.exists() and time.perf_counter() < deadline:
        pump(0.01)
    assert ready.exists(), "Hook probe did not initialize"
    pump(0.25)
    if BASELINE:
        for index in range(5):
            check(f"paused_engine_raw_burst_{index}", edit, ":challa :emoji", ":challa :emoji", True)
        sys.exit(0)
    intro = "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더"
    check("immediate", edit, ":challa", intro)
    check("after_korean_emoji_and_newline", edit, ":challa", "앞선 한글 😀\n다음 문장 " + intro, True, prefix="앞선 한글 😀\r\n다음 문장 ")
    check("burst_with_tail", edit, ":challaXYZ", intro + "XYZ", True)
    check("multiple_burst_expansions", edit, ":challa :emoji", intro + " 안녕 😀", True)
    # Seed Unicode + HTML formats, then verify both survive a delayed restore.
    if original is not None:
        cb.OpenClipboard(window)
        cb.EmptyClipboard()
        cb.SetClipboardText("Challa regression clipboard sentinel", con.CF_UNICODETEXT)
        html = cb.RegisterClipboardFormat("HTML Format")
        cb.SetClipboardData(html, b"<b>clipboard regression</b>\0")
        cb.CloseClipboard()
    check("multiline_signature", edit, ":sig ", "감사합니다.\nChalla 드림")
    check("large_clipboard_payload", edit, ":long", "안전한 긴 문장입니다.\n" * 30, True)
    pump(1.2)
    if original is not None:
        cb.OpenClipboard(window)
        try:
            assert cb.GetClipboardData(con.CF_UNICODETEXT) == "Challa regression clipboard sentinel"
            assert cb.GetClipboardData(html).startswith(b"<b>clipboard regression</b>")
        finally:
            cb.CloseClipboard()
        results.append({"scenario": "clipboard_text_and_html_restored", "passed": True})
    check("signature_with_fast_tail", edit, ":sig XYZ", "감사합니다.\nChalla 드림XYZ", True)
    check("enter_trigger", edit, ":enter\r", "엔터 완료", True)
    check("tab_trigger", edit, ":tab\t", "탭 완료", True)
    check("ordinary_typing", edit, "normal typing 123", "normal typing 123", True)
    set_text(edit, "")
    send([key(con.VK_ESCAPE), key(con.VK_ESCAPE, True)]); pump(0.08)
    send(events(":"))
    send([key(con.VK_LSHIFT)]); pump(0.03)
    send([key(ord("U")), key(ord("U"), True)]); pump(0.03)
    send([key(ord("P")), key(ord("P"), True)])
    pump(0.4)
    assert value(edit) == "대문자 치환", value(edit)
    assert user.GetAsyncKeyState(con.VK_LSHIFT) & 0x8000, "Held Shift was not restored"
    send([key(con.VK_LSHIFT, True)]); pump(0.03)
    send(events("x")); pump(0.1)
    assert value(edit) == "대문자 치환x"
    results.append({"scenario": "held_shift_restored_without_sticking", "passed": True})
    # Mouse/cursor breaks must prevent a stale prefix from erasing unrelated text.
    set_text(edit, "")
    for char in ":cha":
        send(events(char)); pump(0.015)
    pump(0.05)
    send([key(con.VK_LEFT, extended=True), key(con.VK_LEFT, True, True)]); pump(0.03)
    for char in "lla":
        send(events(char)); pump(0.015)
    pump(0.15)
    assert value(edit) == ":chllaa", repr(value(edit))
    results.append({"scenario": "cursor_move_cancels_prefix", "passed": True})
    # Copying while the restore timer is pending must preserve the new user's clipboard.
    check("long_payload_before_external_copy", edit, ":long", "안전한 긴 문장입니다.\n" * 30, True)
    if original is not None:
        cb.OpenClipboard(window); cb.EmptyClipboard(); cb.SetClipboardText("newer clipboard", con.CF_UNICODETEXT); cb.CloseClipboard()
        pump(1.2)
        cb.OpenClipboard(window)
        try:
            assert cb.GetClipboardData(con.CF_UNICODETEXT) == "newer clipboard"
        finally:
            cb.CloseClipboard()
        results.append({"scenario": "newer_clipboard_not_overwritten", "passed": True})
    # Run repeated substitutions to detect thread/timer leakage and ordering regressions.
    for index in range(20):
        check(f"repeat_signature_{index + 1}", edit, ":sig ", "감사합니다.\nChalla 드림", True)
    if NOTEPAD:
        assert gui.SendMessage(ime_window, 0x0283, 0x0005, 0) == 0
        results.append({"scenario": "ime_english_mode_preserved", "passed": True})
        count = user.GetKeyboardLayoutList(0, None)
        layouts = (c.c_void_p * count)()
        user.GetKeyboardLayoutList(count, layouts)
        korean = next((layout for layout in layouts if layout and layout & 0xffff == 0x0412), None)
        if not korean:
            korean = user.LoadKeyboardLayoutW("00000412", 0)
        assert korean, "Korean input locale is required for the composition scenario"
        gui.SendMessage(window, 0x0050, 0, c.c_ssize_t(korean).value)
        pump(0.3)
        assert user.GetKeyboardLayout(target_thread) & 0xffff == 0x0412
        ime_window = imm.ImmGetDefaultIMEWnd(edit)
        gui.SendMessage(ime_window, 0x0283, 0x0006, 1)
        gui.SendMessage(ime_window, 0x0283, 0x0002, 1)
        pump(0.2)
        check("native_korean_composition_is_not_expanded", edit, "rksk ", "가나 ")
    if not NOTEPAD:
        password = gui.CreateWindowEx(0, "EDIT", "", con.WS_CHILD | con.WS_VISIBLE | con.ES_PASSWORD, 10, 320, 700, 30, window, 2, 0, None)
        gui.SetFocus(password); pump(0.2)
        send(events(":challa")); pump(0.2)
        assert value(password) == ":challa", "Password field unexpectedly expanded a keyword"
        results.append({"scenario": "password_field_blocks_capture_and_expansion", "passed": True})
finally:
    # A failed held-modifier assertion must never leave the user's keyboard held.
    send([key(con.VK_LSHIFT, True), key(con.VK_LCONTROL, True), key(con.VK_LMENU, True)])
    pump(0.05)
    stop.write_text("stop", encoding="utf-8")
    if probe:
        probe.wait(timeout=15)
    send([key(con.VK_LSHIFT, True), key(con.VK_LCONTROL, True), key(con.VK_LMENU, True)])
    stdout.close()
    user.ActivateKeyboardLayout(locale, 0)
    if original is not None:
        restore_original_clipboard(window, original)
    if NOTEPAD and window and gui.IsWindow(window) and original_target_layout:
        gui.SendMessage(window, 0x0050, 0, c.c_ssize_t(original_target_layout).value)
        pump(0.1)
        restore_ime = imm.ImmGetDefaultIMEWnd(edit)
        if original_ime_conversion is not None:
            gui.SendMessage(restore_ime, 0x0283, 0x0002, original_ime_conversion)
        if ime_open is not None:
            gui.SendMessage(restore_ime, 0x0283, 0x0006, ime_open)
    if window and not NOTEPAD:
        gui.DestroyWindow(window)
    elif window and gui.GetForegroundWindow() == window and document_label and document_label in gui.GetWindowText(window):
        # Save and close only the owned regression document, preserving other tabs.
        send([key(con.VK_LCONTROL), key(ord("S")), key(ord("S"), True), key(con.VK_LCONTROL, True)])
        pump(0.2)
        send([key(con.VK_LCONTROL), key(ord("W")), key(ord("W"), True), key(con.VK_LCONTROL, True)])
        pump(0.2)
    (OUT / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
