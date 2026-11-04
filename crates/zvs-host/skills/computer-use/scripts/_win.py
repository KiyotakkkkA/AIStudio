import ctypes
import os
import time
from ctypes import wintypes

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
shell32 = ctypes.WinDLL("shell32", use_last_error=True)
dwmapi = ctypes.WinDLL("dwmapi")

ULONG_PTR = ctypes.c_size_t
AGENT_MARK = 0x5A565341

INPUT_MOUSE = 0
INPUT_KEYBOARD = 1
KEYEVENTF_EXTENDEDKEY = 0x0001
KEYEVENTF_KEYUP = 0x0002
KEYEVENTF_UNICODE = 0x0004
MOUSEEVENTF_LEFTDOWN = 0x0002
MOUSEEVENTF_LEFTUP = 0x0004
MOUSEEVENTF_RIGHTDOWN = 0x0008
MOUSEEVENTF_RIGHTUP = 0x0010
MOUSEEVENTF_MIDDLEDOWN = 0x0020
MOUSEEVENTF_MIDDLEUP = 0x0040
MOUSEEVENTF_WHEEL = 0x0800
MOUSEEVENTF_HWHEEL = 0x1000
WHEEL_DELTA = 120
GA_ROOT = 2
GA_ROOTOWNER = 3
GW_OWNER = 4
GWL_EXSTYLE = -20
WS_EX_TOOLWINDOW = 0x00000080
WS_EX_TRANSPARENT = 0x00000020
WS_EX_NOACTIVATE = 0x08000000
SW_RESTORE = 9
SW_SHOWNORMAL = 1
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
DWMWA_CLOAKED = 14


class POINT(ctypes.Structure):
    _fields_ = [("x", wintypes.LONG), ("y", wintypes.LONG)]


class RECT(ctypes.Structure):
    _fields_ = [
        ("left", wintypes.LONG),
        ("top", wintypes.LONG),
        ("right", wintypes.LONG),
        ("bottom", wintypes.LONG),
    ]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [
        ("dx", wintypes.LONG),
        ("dy", wintypes.LONG),
        ("mouseData", wintypes.DWORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    ]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [
        ("wVk", wintypes.WORD),
        ("wScan", wintypes.WORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    ]


class HARDWAREINPUT(ctypes.Structure):
    _fields_ = [
        ("uMsg", wintypes.DWORD),
        ("wParamL", wintypes.WORD),
        ("wParamH", wintypes.WORD),
    ]


class _INPUTUNION(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT), ("hi", HARDWAREINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("u", _INPUTUNION)]


WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

user32.WindowFromPoint.argtypes = [POINT]
user32.WindowFromPoint.restype = wintypes.HWND
user32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
user32.GetAncestor.restype = wintypes.HWND
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(RECT)]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.IsIconic.argtypes = [wintypes.HWND]
user32.IsWindow.argtypes = [wintypes.HWND]
user32.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
user32.GetWindow.restype = wintypes.HWND
user32.GetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetWindowLongPtrW.restype = ctypes.c_ssize_t
user32.EnumWindows.argtypes = [WNDENUMPROC, wintypes.LPARAM]
user32.EnumChildWindows.argtypes = [wintypes.HWND, WNDENUMPROC, wintypes.LPARAM]
user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
user32.GetCursorPos.argtypes = [ctypes.POINTER(POINT)]
user32.SendInput.argtypes = [wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
user32.SendInput.restype = wintypes.UINT
user32.MapVirtualKeyW.argtypes = [wintypes.UINT, wintypes.UINT]
user32.MapVirtualKeyW.restype = wintypes.UINT
user32.VkKeyScanW.argtypes = [wintypes.WCHAR]
user32.VkKeyScanW.restype = ctypes.c_short
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
user32.GetDoubleClickTime.restype = wintypes.UINT
kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
kernel32.QueryFullProcessImageNameW.argtypes = [
    wintypes.HANDLE,
    wintypes.DWORD,
    wintypes.LPWSTR,
    ctypes.POINTER(wintypes.DWORD),
]
kernel32.GetCurrentThreadId.restype = wintypes.DWORD
shell32.ShellExecuteW.argtypes = [
    wintypes.HWND,
    wintypes.LPCWSTR,
    wintypes.LPCWSTR,
    wintypes.LPCWSTR,
    wintypes.LPCWSTR,
    ctypes.c_int,
]
shell32.ShellExecuteW.restype = wintypes.HINSTANCE
dwmapi.DwmGetWindowAttribute.argtypes = [
    wintypes.HWND,
    wintypes.DWORD,
    ctypes.c_void_p,
    wintypes.DWORD,
]


def enable_dpi_awareness() -> None:
    try:
        user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except (AttributeError, OSError):
        try:
            ctypes.WinDLL("shcore").SetProcessDpiAwareness(2)
        except (AttributeError, OSError):
            user32.SetProcessDPIAware()


def process_path(pid: int) -> str:
    handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not handle:
        return ""
    try:
        size = wintypes.DWORD(1024)
        buffer = ctypes.create_unicode_buffer(size.value)
        if kernel32.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
            return buffer.value
        return ""
    finally:
        kernel32.CloseHandle(handle)


def window_pid(hwnd) -> int:
    pid = wintypes.DWORD(0)
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def window_title(hwnd) -> str:
    length = user32.GetWindowTextLengthW(hwnd)
    if length <= 0:
        return ""
    buffer = ctypes.create_unicode_buffer(length + 1)
    user32.GetWindowTextW(hwnd, buffer, length + 1)
    return buffer.value


def window_class(hwnd) -> str:
    buffer = ctypes.create_unicode_buffer(256)
    user32.GetClassNameW(hwnd, buffer, 256)
    return buffer.value


def window_rect(hwnd) -> tuple[int, int, int, int]:
    rect = RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    return rect.left, rect.top, rect.right, rect.bottom


def is_cloaked(hwnd) -> bool:
    cloaked = wintypes.DWORD(0)
    result = dwmapi.DwmGetWindowAttribute(
        hwnd, DWMWA_CLOAKED, ctypes.byref(cloaked), ctypes.sizeof(cloaked)
    )
    return result == 0 and cloaked.value != 0


def ex_style(hwnd) -> int:
    return int(user32.GetWindowLongPtrW(hwnd, GWL_EXSTYLE))


def root_window(hwnd):
    if not hwnd:
        return None
    return user32.GetAncestor(hwnd, GA_ROOT) or hwnd


FRAME_HOST = "applicationframehost.exe"


def hosted_pid(hwnd, host_pid: int) -> int:
    found = []

    def collect(child, _):
        pid = window_pid(child)
        if pid != host_pid:
            found.append(pid)
            return False
        return True

    user32.EnumChildWindows(hwnd, WNDENUMPROC(collect), 0)
    return found[0] if found else host_pid


def describe(hwnd) -> dict:
    if not hwnd:
        return {"hwnd": 0, "pid": 0, "exe": "", "path": "", "title": "", "className": ""}
    pid = window_pid(hwnd)
    path = process_path(pid)
    if os.path.basename(path).lower() == FRAME_HOST:
        pid = hosted_pid(hwnd, pid)
        path = process_path(pid)
    return {
        "hwnd": int(hwnd),
        "pid": pid,
        "exe": os.path.basename(path).lower(),
        "path": path,
        "title": window_title(hwnd),
        "className": window_class(hwnd),
    }


def window_at(x: int, y: int):
    hwnd = user32.WindowFromPoint(POINT(x, y))
    return root_window(hwnd)


def foreground_window():
    return root_window(user32.GetForegroundWindow())


def cursor_position() -> tuple[int, int]:
    point = POINT()
    user32.GetCursorPos(ctypes.byref(point))
    return point.x, point.y


def top_windows() -> list:
    found = []

    def collect(hwnd, _):
        if not user32.IsWindowVisible(hwnd) or is_cloaked(hwnd):
            return True
        if user32.GetWindow(hwnd, GW_OWNER):
            return True
        if ex_style(hwnd) & WS_EX_TOOLWINDOW:
            return True
        if not window_title(hwnd):
            return True
        found.append(hwnd)
        return True

    user32.EnumWindows(WNDENUMPROC(collect), 0)
    return found


def focus_window(hwnd) -> bool:
    if not user32.IsWindow(hwnd):
        return False
    if user32.IsIconic(hwnd):
        user32.ShowWindow(hwnd, SW_RESTORE)
    current = kernel32.GetCurrentThreadId()
    target = user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), None)
    attached = target and target != current and user32.AttachThreadInput(current, target, True)
    try:
        user32.BringWindowToTop(hwnd)
        user32.SetForegroundWindow(hwnd)
    finally:
        if attached:
            user32.AttachThreadInput(current, target, False)
    time.sleep(0.15)
    return foreground_window() == root_window(hwnd)


def shell_open(target: str) -> bool:
    result = shell32.ShellExecuteW(None, "open", target, None, None, SW_SHOWNORMAL)
    return (result or 0) > 32


def _send(inputs: list) -> None:
    if not inputs:
        return
    array = (INPUT * len(inputs))(*inputs)
    sent = user32.SendInput(len(inputs), array, ctypes.sizeof(INPUT))
    if sent != len(inputs):
        raise OSError(ctypes.get_last_error(), "SendInput was blocked")


def _mouse(flags: int, data: int = 0) -> INPUT:
    item = INPUT(type=INPUT_MOUSE)
    item.u.mi = MOUSEINPUT(0, 0, ctypes.c_ulong(data & 0xFFFFFFFF).value, flags, 0, AGENT_MARK)
    return item


def _key(vk: int, up: bool, extended: bool) -> INPUT:
    flags = (KEYEVENTF_KEYUP if up else 0) | (KEYEVENTF_EXTENDEDKEY if extended else 0)
    item = INPUT(type=INPUT_KEYBOARD)
    item.u.ki = KEYBDINPUT(vk, user32.MapVirtualKeyW(vk, 0), flags, 0, AGENT_MARK)
    return item


def _unicode(unit: int, up: bool) -> INPUT:
    flags = KEYEVENTF_UNICODE | (KEYEVENTF_KEYUP if up else 0)
    item = INPUT(type=INPUT_KEYBOARD)
    item.u.ki = KEYBDINPUT(0, unit, flags, 0, AGENT_MARK)
    return item


BUTTONS = {
    "left": (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
    "right": (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
    "middle": (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
}


def move_to(x: int, y: int, steps: int = 8) -> None:
    start_x, start_y = cursor_position()
    for step in range(1, steps + 1):
        user32.SetCursorPos(
            round(start_x + (x - start_x) * step / steps),
            round(start_y + (y - start_y) * step / steps),
        )
        time.sleep(0.008)


def button_down(button: str) -> None:
    _send([_mouse(BUTTONS[button][0])])


def button_up(button: str) -> None:
    _send([_mouse(BUTTONS[button][1])])


def click(button: str, count: int) -> None:
    pause = min(user32.GetDoubleClickTime() / 1000 / 4, 0.08)
    down, up = BUTTONS[button]
    for index in range(count):
        _send([_mouse(down)])
        time.sleep(0.02)
        _send([_mouse(up)])
        if index + 1 < count:
            time.sleep(pause)


def wheel(vertical: int, horizontal: int) -> None:
    for _ in range(abs(vertical)):
        _send([_mouse(MOUSEEVENTF_WHEEL, -WHEEL_DELTA if vertical > 0 else WHEEL_DELTA)])
        time.sleep(0.03)
    for _ in range(abs(horizontal)):
        _send([_mouse(MOUSEEVENTF_HWHEEL, WHEEL_DELTA if horizontal > 0 else -WHEEL_DELTA)])
        time.sleep(0.03)


NAMED_KEYS = {
    "enter": 0x0D,
    "return": 0x0D,
    "tab": 0x09,
    "escape": 0x1B,
    "esc": 0x1B,
    "backspace": 0x08,
    "delete": 0x2E,
    "del": 0x2E,
    "insert": 0x2D,
    "home": 0x24,
    "end": 0x23,
    "pageup": 0x21,
    "pagedown": 0x22,
    "left": 0x25,
    "up": 0x26,
    "right": 0x27,
    "down": 0x28,
    "space": 0x20,
    "ctrl": 0x11,
    "control": 0x11,
    "shift": 0x10,
    "alt": 0x12,
    "win": 0x5B,
    "meta": 0x5B,
    "super": 0x5B,
    "cmd": 0x5B,
    "capslock": 0x14,
    "printscreen": 0x2C,
    "menu": 0x5D,
    "apps": 0x5D,
    "numlock": 0x90,
    "scrolllock": 0x91,
    "pause": 0x13,
}
NAMED_KEYS.update({f"f{number}": 0x6F + number for number in range(1, 25)})
EXTENDED = {0x2E, 0x2D, 0x24, 0x23, 0x21, 0x22, 0x25, 0x26, 0x27, 0x28, 0x5B, 0x5D, 0x2C, 0x90}
ALIASES = {
    "pgup": "pageup",
    "pgdn": "pagedown",
    "arrowleft": "left",
    "arrowright": "right",
    "arrowup": "up",
    "arrowdown": "down",
    "page_up": "pageup",
    "page_down": "pagedown",
}


def virtual_key(name: str) -> int:
    lowered = name.strip().lower()
    lowered = ALIASES.get(lowered, lowered)
    if lowered in NAMED_KEYS:
        return NAMED_KEYS[lowered]
    if len(name.strip()) == 1:
        char = name.strip()
        if char.isascii() and char.isalnum():
            return ord(char.upper())
        scanned = user32.VkKeyScanW(char)
        if scanned != -1:
            return scanned & 0xFF
    raise ValueError(f"unknown key: {name}")


def parse_combo(combo: str) -> list:
    text = combo.replace(" ", "")
    if not text:
        raise ValueError("empty key combination")
    names = [part for part in text.split("+") if part]
    if text.endswith("+"):
        names.append("+")
    return [virtual_key(name) for name in names]


def press_combo(keys: list) -> None:
    downs = [_key(vk, False, vk in EXTENDED) for vk in keys]
    ups = [_key(vk, True, vk in EXTENDED) for vk in reversed(keys)]
    _send(downs)
    time.sleep(0.03)
    _send(ups)


def type_text(text: str) -> None:
    for char in text:
        if char == "\n":
            press_combo([0x0D])
        elif char == "\t":
            press_combo([0x09])
        elif char == "\r":
            continue
        else:
            encoded = char.encode("utf-16-le")
            units = [int.from_bytes(encoded[i : i + 2], "little") for i in range(0, len(encoded), 2)]
            _send([_unicode(unit, False) for unit in units] + [_unicode(unit, True) for unit in units])
        time.sleep(0.004)
