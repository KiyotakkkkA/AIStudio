#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock, PoisonError};

use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LSHIFT};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GWL_EXSTYLE, GetMessageW, GetWindowLongPtrW, HC_ACTION,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SW_SHOWNOACTIVATE, SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowsHookExW, ShowWindow,
    TranslateMessage, UnhookWindowsHookEx, WDA_EXCLUDEFROMCAPTURE, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_QUIT, WM_SYSKEYDOWN, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

type Trigger = Box<dyn Fn() + Send>;

static TRIGGER: OnceLock<Mutex<Option<Trigger>>> = OnceLock::new();
static THREAD: Mutex<Option<u32>> = Mutex::new(None);

fn handle(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

pub fn hide_from_capture(raw: isize) -> bool {
    unsafe { SetWindowDisplayAffinity(handle(raw), WDA_EXCLUDEFROMCAPTURE).is_ok() }
}

pub fn set_no_activate(raw: isize, enabled: bool) {
    let window = handle(raw);
    unsafe {
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let flags = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        let next = if enabled {
            style | flags
        } else {
            style & !(WS_EX_NOACTIVATE.0 as isize)
        };
        SetWindowLongPtrW(window, GWL_EXSTYLE, next);
    }
}

pub fn show_inactive(raw: isize) {
    unsafe {
        let _ = ShowWindow(handle(raw), SW_SHOWNOACTIVATE);
    }
}

fn fire() {
    if let Some(slot) = TRIGGER.get()
        && let Some(trigger) = slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref()
    {
        trigger();
    }
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let pressed = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
    if code == HC_ACTION as i32 && pressed {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = info.flags.0 & LLKHF_INJECTED.0 != 0;
        let shift = unsafe { GetAsyncKeyState(i32::from(VK_LSHIFT.0)) } as u16 & 0x8000 != 0;
        if !injected && shift && info.vkCode == u32::from(VK_ESCAPE.0) {
            fire();
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub fn watch_stop_key(trigger: impl Fn() + Send + 'static) {
    release_stop_key();
    *TRIGGER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(trigger));
    let (ready, started) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("computer-stop-key".to_owned())
        .spawn(move || unsafe {
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
            let module = GetModuleHandleW(None).ok();
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module.map(Into::into), 0);
            let _ = ready.send(GetCurrentThreadId());
            let Ok(hook) = hook else {
                return;
            };
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let _ = UnhookWindowsHookEx(hook);
        });
    if spawned.is_ok()
        && let Ok(thread) = started.recv()
    {
        *THREAD.lock().unwrap_or_else(PoisonError::into_inner) = Some(thread);
    }
}

pub fn release_stop_key() {
    if let Some(thread) = THREAD.lock().unwrap_or_else(PoisonError::into_inner).take() {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    if let Some(slot) = TRIGGER.get() {
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}
