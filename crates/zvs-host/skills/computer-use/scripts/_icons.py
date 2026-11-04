import base64
import ctypes
import io
import os
import shutil
import winreg
from ctypes import wintypes

from PIL import Image

import _win

gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
ICON_SIZE = 32
DI_NORMAL = 3
APP_PATHS = r"Software\Microsoft\Windows\CurrentVersion\App Paths"


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


_win.shell32.ExtractIconExW.argtypes = [
    wintypes.LPCWSTR,
    ctypes.c_int,
    ctypes.POINTER(wintypes.HICON),
    ctypes.POINTER(wintypes.HICON),
    wintypes.UINT,
]
_win.shell32.ExtractIconExW.restype = wintypes.UINT
_win.user32.DrawIconEx.argtypes = [
    wintypes.HDC,
    ctypes.c_int,
    ctypes.c_int,
    wintypes.HICON,
    ctypes.c_int,
    ctypes.c_int,
    wintypes.UINT,
    wintypes.HBRUSH,
    wintypes.UINT,
]
_win.user32.DestroyIcon.argtypes = [wintypes.HICON]
_win.user32.GetDC.argtypes = [wintypes.HWND]
_win.user32.GetDC.restype = wintypes.HDC
_win.user32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
gdi32.CreateCompatibleDC.argtypes = [wintypes.HDC]
gdi32.CreateCompatibleDC.restype = wintypes.HDC
gdi32.CreateDIBSection.argtypes = [
    wintypes.HDC,
    ctypes.c_void_p,
    wintypes.UINT,
    ctypes.POINTER(ctypes.c_void_p),
    wintypes.HANDLE,
    wintypes.DWORD,
]
gdi32.CreateDIBSection.restype = wintypes.HBITMAP
gdi32.SelectObject.argtypes = [wintypes.HDC, wintypes.HGDIOBJ]
gdi32.SelectObject.restype = wintypes.HGDIOBJ
gdi32.DeleteObject.argtypes = [wintypes.HGDIOBJ]
gdi32.DeleteDC.argtypes = [wintypes.HDC]


def app_path(app: str) -> str:
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        try:
            with winreg.OpenKey(hive, rf"{APP_PATHS}\{app}") as key:
                value, _ = winreg.QueryValueEx(key, "")
                value = os.path.expandvars(str(value)).strip('"')
                if os.path.isfile(value):
                    return value
        except OSError:
            continue
    return shutil.which(app) or ""


def icon_png(path: str) -> str | None:
    if not path or not os.path.isfile(path):
        return None
    large = wintypes.HICON()
    if not _win.shell32.ExtractIconExW(path, 0, ctypes.byref(large), None, 1) or not large:
        return None
    screen = _win.user32.GetDC(None)
    memory = gdi32.CreateCompatibleDC(screen)
    header = BITMAPINFOHEADER(
        ctypes.sizeof(BITMAPINFOHEADER), ICON_SIZE, -ICON_SIZE, 1, 32, 0, 0, 0, 0, 0, 0
    )
    bits = ctypes.c_void_p()
    bitmap = gdi32.CreateDIBSection(memory, ctypes.byref(header), 0, ctypes.byref(bits), None, 0)
    try:
        if not bitmap:
            return None
        previous = gdi32.SelectObject(memory, bitmap)
        _win.user32.DrawIconEx(memory, 0, 0, large, ICON_SIZE, ICON_SIZE, 0, None, DI_NORMAL)
        gdi32.SelectObject(memory, previous)
        raw = ctypes.string_at(bits, ICON_SIZE * ICON_SIZE * 4)
        image = Image.frombuffer("RGBA", (ICON_SIZE, ICON_SIZE), raw, "raw", "BGRA", 0, 1)
        if image.getchannel("A").getextrema()[1] == 0:
            image.putalpha(255)
        buffer = io.BytesIO()
        image.save(buffer, "PNG", optimize=True)
        return base64.b64encode(buffer.getvalue()).decode("ascii")
    finally:
        if bitmap:
            gdi32.DeleteObject(bitmap)
        gdi32.DeleteDC(memory)
        _win.user32.ReleaseDC(None, screen)
        _win.user32.DestroyIcon(large)


def resolve(entries: list) -> list:
    running = {}
    for hwnd in _win.top_windows():
        described = _win.describe(hwnd)
        if described["exe"] and described["path"]:
            running.setdefault(described["exe"], described["path"])
    found = []
    for entry in entries:
        app = entry["app"]
        path = entry.get("path") or running.get(app) or app_path(app)
        found.append({"app": app, "path": path, "icon": icon_png(path)})
    return found
