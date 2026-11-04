# /// script
# requires-python = ">=3.11"
# dependencies = ["mss==10.2.0", "pillow==13.0.0", "uiautomation==2.0.29"]
# ///
import base64
import io
import json
import os
import sys
import time
import traceback

import _win

_win.enable_dpi_awareness()

import mss
from PIL import Image

import _icons
import _uia

PROTOCOL = 1
SHOT_WIDTH = 1280
SHOT_HEIGHT = 800
JPEG_QUALITY = 80
POINTER_ACTIONS = {"click", "scroll", "drag", "move"}
KEYBOARD_ACTIONS = {"type", "key"}
LAUNCH_SECONDS = 15.0


class Refused(Exception):
    def __init__(self, code: str, message: str, details: dict | None = None):
        super().__init__(message)
        self.code = code
        self.details = details or {}


def fit(width: int, height: int, max_width: int, max_height: int) -> tuple[int, int]:
    ratio = min(max_width / width, max_height / height, 1.0)
    return max(1, round(width * ratio)), max(1, round(height * ratio))


class Desktop:
    def __init__(self):
        self.capture = mss.MSS()
        self.index = self.primary_index()
        self.refs = {}

    def monitors(self) -> list:
        return self.capture.monitors[1:]

    def primary_index(self) -> int:
        for index, monitor in enumerate(self.monitors()):
            if monitor["left"] == 0 and monitor["top"] == 0:
                return index
        return 0

    def display(self, index: int | None = None) -> dict:
        index = self.index if index is None else index
        monitor = self.monitors()[index]
        shot_width, shot_height = fit(monitor["width"], monitor["height"], SHOT_WIDTH, SHOT_HEIGHT)
        return {
            "index": index,
            "left": monitor["left"],
            "top": monitor["top"],
            "width": monitor["width"],
            "height": monitor["height"],
            "shotWidth": shot_width,
            "shotHeight": shot_height,
            "primary": index == self.primary_index(),
        }

    def displays(self) -> list:
        return [self.display(index) for index in range(len(self.monitors()))]

    def physical(self, x, y) -> tuple[int, int]:
        display = self.display()
        if not (0 <= x < display["shotWidth"] and 0 <= y < display["shotHeight"]):
            raise Refused(
                "out_of_bounds",
                f"point {x},{y} is outside the screenshot {display['shotWidth']}x{display['shotHeight']}",
            )
        return (
            display["left"] + round(x * display["width"] / display["shotWidth"]),
            display["top"] + round(y * display["height"] / display["shotHeight"]),
        )

    def logical(self, px: int, py: int) -> tuple[int, int]:
        display = self.display()
        return (
            round((px - display["left"]) * display["shotWidth"] / display["width"]),
            round((py - display["top"]) * display["shotHeight"] / display["height"]),
        )

    def on_display(self, rect) -> bool:
        display = self.display()
        return (
            rect.right > display["left"]
            and rect.bottom > display["top"]
            and rect.left < display["left"] + display["width"]
            and rect.top < display["top"] + display["height"]
        )

    def point(self, action: dict, key: str = "") -> tuple[int, int]:
        ref = action.get(f"{key}ref" if key else "ref")
        if ref:
            target = self.refs.get(ref)
            if target is None:
                raise Refused("bad_ref", f"unknown element reference {ref}; call inspect again")
            return target
        return self.physical(action["x"], action["y"])

    def cursor(self):
        px, py = _win.cursor_position()
        display = self.display()
        inside = (
            display["left"] <= px < display["left"] + display["width"]
            and display["top"] <= py < display["top"] + display["height"]
        )
        if not inside:
            return None
        x, y = self.logical(px, py)
        return {"x": x, "y": y}

    def encode(self, image) -> str:
        buffer = io.BytesIO()
        image.save(buffer, "JPEG", quality=JPEG_QUALITY, optimize=True)
        return base64.b64encode(buffer.getvalue()).decode("ascii")

    def grab(self, region: dict):
        shot = self.capture.grab(region)
        return Image.frombytes("RGB", shot.size, shot.bgra, "raw", "BGRX")

    def screenshot(self, params: dict) -> dict:
        if params.get("display") is not None:
            index = int(params["display"])
            if not 0 <= index < len(self.monitors()):
                raise Refused("bad_display", f"display {index} does not exist")
            self.index = index
            self.refs = {}
        display = self.display()
        image = self.grab(self.monitors()[self.index])
        image = image.resize((display["shotWidth"], display["shotHeight"]), Image.LANCZOS)
        return {
            "data": self.encode(image),
            "width": display["shotWidth"],
            "height": display["shotHeight"],
            "display": display,
            "displays": len(self.monitors()),
            "cursor": self.cursor(),
            "foreground": _win.describe(_win.foreground_window()),
        }

    def zoom(self, params: dict) -> dict:
        x, y, width, height = (int(params[key]) for key in ("x", "y", "width", "height"))
        left, top = self.physical(x, y)
        right, bottom = self.physical(
            min(x + width, self.display()["shotWidth"] - 1),
            min(y + height, self.display()["shotHeight"] - 1),
        )
        if right - left < 4 or bottom - top < 4:
            raise Refused("bad_region", "the zoom region is too small")
        image = self.grab({"left": left, "top": top, "width": right - left, "height": bottom - top})
        size = fit(image.width, image.height, SHOT_WIDTH, SHOT_HEIGHT)
        if size != image.size:
            image = image.resize(size, Image.LANCZOS)
        return {"data": self.encode(image), "width": image.width, "height": image.height}

    def window_entry(self, hwnd, foreground) -> dict:
        entry = _win.describe(hwnd)
        left, top, right, bottom = _win.window_rect(hwnd)
        entry["foreground"] = hwnd == foreground
        entry["minimized"] = bool(_win.user32.IsIconic(hwnd))
        display = self.display()
        visible = (
            right > display["left"]
            and bottom > display["top"]
            and left < display["left"] + display["width"]
            and top < display["top"] + display["height"]
        )
        entry["onDisplay"] = visible and not entry["minimized"]
        if entry["onDisplay"]:
            x, y = self.logical(max(left, display["left"]), max(top, display["top"]))
            x2, y2 = self.logical(
                min(right, display["left"] + display["width"]),
                min(bottom, display["top"] + display["height"]),
            )
            entry["rect"] = {"x": x, "y": y, "width": x2 - x, "height": y2 - y}
        return entry

    def windows(self, _params: dict) -> dict:
        foreground = _win.foreground_window()
        return {
            "windows": [self.window_entry(hwnd, foreground) for hwnd in _win.top_windows()],
            "display": self.display(),
            "displays": self.displays(),
        }

    def target_at(self, px: int, py: int) -> dict:
        return _win.describe(_win.window_at(px, py))

    def probe(self, params: dict) -> dict:
        predicted = _win.describe(_win.foreground_window())
        targets = []
        for action in params["actions"]:
            kind = action["type"]
            if kind in {"click", "scroll", "drag"}:
                window = self.target_at(*self.point(action))
                if kind == "drag":
                    end = self.physical(*action["path"][-1])
                    targets.append({"kind": "pointer", "window": window, "end": self.target_at(*end)})
                else:
                    targets.append({"kind": "pointer", "window": window})
                if kind == "click" or kind == "drag":
                    predicted = window
            elif kind in KEYBOARD_ACTIONS:
                targets.append({"kind": "keyboard", "window": predicted})
            else:
                targets.append({"kind": "none"})
        return {"targets": targets}

    def check(self, window: dict, params: dict) -> None:
        if window["pid"] in params.get("deniedPids", []):
            raise Refused("own_window", "the point is covered by a window of the app itself", window)
        if window["exe"] not in params.get("allowed", []):
            raise Refused("not_allowed", f"{window['exe'] or 'this window'} is not allowed", window)

    def act(self, params: dict) -> dict:
        done = []
        for index, action in enumerate(params["actions"]):
            try:
                done.append(self.perform(action, params))
            except Refused as refused:
                return {"done": done, "failed": index, "error": refused.code, "message": str(refused), "window": refused.details}
            except (OSError, ValueError, KeyError) as error:
                return {"done": done, "failed": index, "error": "failed", "message": str(error), "window": {}}
        time.sleep(float(params.get("settle", 0.5)))
        return {"done": done}

    def perform(self, action: dict, params: dict) -> dict:
        kind = action["type"]
        if kind == "wait":
            time.sleep(min(int(action.get("ms", 1000)), 10_000) / 1000)
            return {"type": kind}
        if kind == "move":
            _win.move_to(*self.point(action))
            return {"type": kind}
        if kind == "click":
            px, py = self.point(action)
            self.check(self.target_at(px, py), params)
            modifiers = [_win.virtual_key(name) for name in action.get("modifiers", [])]
            _win.move_to(px, py)
            if modifiers:
                _win._send([_win._key(vk, False, vk in _win.EXTENDED) for vk in modifiers])
            try:
                _win.click(action.get("button", "left"), int(action.get("count", 1)))
            finally:
                if modifiers:
                    _win._send([_win._key(vk, True, vk in _win.EXTENDED) for vk in reversed(modifiers)])
            return {"type": kind}
        if kind == "scroll":
            px, py = self.point(action)
            self.check(self.target_at(px, py), params)
            _win.move_to(px, py, steps=2)
            _win.wheel(int(action.get("scrollY", 0)), int(action.get("scrollX", 0)))
            return {"type": kind}
        if kind == "drag":
            points = [self.physical(x, y) for x, y in action["path"]]
            self.check(self.target_at(*points[0]), params)
            self.check(self.target_at(*points[-1]), params)
            button = action.get("button", "left")
            _win.move_to(*points[0])
            _win.button_down(button)
            try:
                for point in points[1:]:
                    _win.move_to(*point, steps=12)
            finally:
                _win.button_up(button)
            return {"type": kind}
        if kind in KEYBOARD_ACTIONS:
            window = _win.describe(_win.foreground_window())
            self.check(window, params)
            if kind == "type":
                if _uia.focused_password():
                    raise Refused("password", "the focused field is a password field", window)
                _win.type_text(action["text"])
            else:
                _win.press_combo(_win.parse_combo(action["keys"]))
            return {"type": kind}
        raise Refused("bad_action", f"unknown action {kind}")

    def inspect(self, params: dict) -> dict:
        hwnd = params.get("hwnd") or _win.foreground_window()
        if not hwnd:
            raise Refused("no_window", "there is no foreground window")
        window = _win.describe(hwnd)
        self.check(window, params)
        elements, truncated = _uia.walk(int(hwnd), self.on_display, params.get("query", ""), params.get("role", ""))
        self.refs = {}
        listed = []
        for number, element in enumerate(elements, start=1):
            left, top, right, bottom = element.pop("rect")
            ref = f"e{number}"
            center = ((left + right) // 2, (top + bottom) // 2)
            self.refs[ref] = center
            x, y = self.logical(*center)
            x1, y1 = self.logical(left, top)
            x2, y2 = self.logical(right, bottom)
            listed.append({"ref": ref, "x": x, "y": y, "width": x2 - x1, "height": y2 - y1, **element})
        return {"window": window, "elements": listed, "truncated": truncated}

    def launch(self, params: dict) -> dict:
        exe = params["exe"]
        before = set(int(hwnd) for hwnd in _win.top_windows())
        running = [hwnd for hwnd in _win.top_windows() if _win.describe(hwnd)["exe"] == exe]
        if running and not params.get("newWindow"):
            _win.focus_window(running[0])
            return {"window": _win.describe(running[0]), "launched": False}
        if not _win.shell_open(params.get("target") or exe):
            raise Refused("launch_failed", f"Windows could not start {exe}")
        deadline = time.monotonic() + LAUNCH_SECONDS
        while time.monotonic() < deadline:
            time.sleep(0.3)
            fresh = [hwnd for hwnd in _win.top_windows() if int(hwnd) not in before]
            matching = [hwnd for hwnd in fresh if _win.describe(hwnd)["exe"] == exe]
            chosen = matching or fresh
            if chosen:
                time.sleep(0.4)
                _win.focus_window(chosen[0])
                return {"window": _win.describe(chosen[0]), "launched": True}
        raise Refused("launch_timeout", f"{exe} started but showed no window")

    def resolve(self, params: dict) -> dict:
        return {"apps": _icons.resolve(params.get("apps", []))}

    def focus(self, params: dict) -> dict:
        hwnd = params["hwnd"]
        window = _win.describe(hwnd)
        self.check(window, params)
        if not _win.focus_window(hwnd):
            raise Refused("focus_failed", "Windows did not bring the window to the front", window)
        return {"window": window}


def hello(desktop: Desktop, _params: dict) -> dict:
    return {
        "protocol": PROTOCOL,
        "pid": os.getpid(),
        "display": desktop.display(),
        "displays": desktop.displays(),
    }


def serve() -> None:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stdin.reconfigure(encoding="utf-8")
    desktop = Desktop()
    methods = {
        "hello": lambda params: hello(desktop, params),
        "screenshot": desktop.screenshot,
        "zoom": desktop.zoom,
        "windows": desktop.windows,
        "probe": desktop.probe,
        "act": desktop.act,
        "inspect": desktop.inspect,
        "launch": desktop.launch,
        "focus": desktop.focus,
        "resolve": desktop.resolve,
    }
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        identifier = None
        try:
            request = json.loads(line)
            identifier = request.get("id")
            method = methods.get(request.get("method"))
            if method is None:
                raise Refused("bad_method", f"unknown method {request.get('method')}")
            reply = {"id": identifier, "ok": True, "result": method(request.get("params") or {})}
        except Refused as refused:
            reply = {
                "id": identifier,
                "ok": False,
                "error": {"code": refused.code, "message": str(refused), "window": refused.details},
            }
        except Exception as error:
            traceback.print_exc(file=sys.stderr)
            reply = {"id": identifier, "ok": False, "error": {"code": "failed", "message": str(error)}}
        sys.stdout.write(json.dumps(reply, ensure_ascii=False) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    serve()
