import time

import uiautomation as auto

ELEMENTS_MAX = 250
DEPTH_MAX = 40
BUDGET_SECONDS = 4.0
NAME_CHARS = 120
VALUE_CHARS = 200

ACTIONABLE = {
    "button",
    "edit",
    "checkbox",
    "combobox",
    "hyperlink",
    "listitem",
    "menuitem",
    "radiobutton",
    "tabitem",
    "treeitem",
    "splitbutton",
    "slider",
    "spinner",
    "dataitem",
    "headeritem",
    "document",
    "text",
    "image",
}


def role_of(control) -> str:
    name = control.ControlTypeName or ""
    if name.endswith("Control"):
        name = name[: -len("Control")]
    return name.lower()


def printable(char: str) -> bool:
    code = ord(char)
    return not (0xE000 <= code <= 0xF8FF or code in (0x200E, 0x200F, 0x200B, 0xFEFF))


def short(text: str, limit: int) -> str:
    collapsed = " ".join("".join(filter(printable, text or "")).split())
    return collapsed if len(collapsed) <= limit else collapsed[:limit] + "…"


def is_password(control) -> bool:
    try:
        return bool(control.IsPassword)
    except Exception:
        return False


def value_of(control, role: str) -> str:
    if role not in {"edit", "combobox", "document", "spinner", "slider"}:
        return ""
    if is_password(control):
        return ""
    try:
        pattern = control.GetValuePattern()
        return short(pattern.Value or "", VALUE_CHARS) if pattern else ""
    except Exception:
        return ""


def state_of(control, role: str) -> list:
    states = []
    try:
        if not control.IsEnabled:
            states.append("disabled")
    except Exception:
        pass
    if role in {"checkbox", "radiobutton"}:
        try:
            pattern = control.GetTogglePattern()
            if pattern and pattern.ToggleState == 1:
                states.append("checked")
        except Exception:
            try:
                pattern = control.GetSelectionItemPattern()
                if pattern and pattern.IsSelected:
                    states.append("checked")
            except Exception:
                pass
    try:
        if control.HasKeyboardFocus:
            states.append("focused")
    except Exception:
        pass
    if is_password(control):
        states.append("password")
    return states


def children(control) -> list:
    found = []
    try:
        child = control.GetFirstChildControl()
        while child is not None:
            found.append(child)
            child = child.GetNextSiblingControl()
    except Exception:
        pass
    return found


def walk(hwnd: int, bounds, query: str, role: str) -> tuple[list, bool]:
    root = auto.ControlFromHandle(hwnd)
    if root is None:
        return [], False
    deadline = time.monotonic() + BUDGET_SECONDS
    needle = query.strip().lower()
    wanted = role.strip().lower()
    elements = []
    truncated = False
    stack = [(root, 0)]
    while stack:
        if time.monotonic() > deadline or len(elements) >= ELEMENTS_MAX:
            truncated = True
            break
        control, depth = stack.pop()
        try:
            rect = control.BoundingRectangle
            offscreen = control.IsOffscreen
        except Exception:
            continue
        kind = role_of(control)
        name = short(control.Name or "", NAME_CHARS)
        width = rect.right - rect.left
        height = rect.bottom - rect.top
        visible = not offscreen and width > 1 and height > 1 and bounds(rect)
        if visible and kind in ACTIONABLE and (name or kind in {"edit", "document", "combobox"}):
            haystack = f"{name} {control.AutomationId or ''}".lower()
            if (not needle or needle in haystack) and (not wanted or wanted == kind):
                elements.append(
                    {
                        "role": kind,
                        "name": name,
                        "value": value_of(control, kind),
                        "states": state_of(control, kind),
                        "rect": (rect.left, rect.top, rect.right, rect.bottom),
                        "password": is_password(control),
                    }
                )
        if depth < DEPTH_MAX and (visible or kind in {"window", "pane", "group", "custom"}):
            for child in reversed(children(control)):
                stack.append((child, depth + 1))
    return elements, truncated


def password_at(x: int, y: int) -> bool:
    try:
        control = auto.ControlFromPoint(x, y)
        return control is not None and is_password(control)
    except Exception:
        return False


def focused_password() -> bool:
    try:
        control = auto.GetFocusedControl()
        return control is not None and is_password(control)
    except Exception:
        return False
