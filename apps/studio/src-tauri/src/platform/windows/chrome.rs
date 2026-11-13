#![allow(unsafe_code)]

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DwmSetWindowAttribute};
use windows::Win32::UI::Controls::{
    SetWindowThemeAttribute, WTA_NONCLIENT, WTA_OPTIONS, WTNCA_NODRAWICON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, ICON_SMALL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowLongPtrW, SetWindowPos, WM_SETICON,
    WS_EX_DLGMODALFRAME,
};

use crate::studio_chrome::CaptionColors;

fn handle(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

pub fn hide_caption_icon(raw: isize) {
    let window = handle(raw);
    // Suppress the caption image explicitly; a frame style alone can still show
    // the small icon assigned by Tauri. Keep the large taskbar / Alt+Tab icon.
    // SAFETY: the caller supplies a live Studio HWND and runs on its UI thread.
    unsafe {
        let options = WTA_OPTIONS {
            dwFlags: WTNCA_NODRAWICON,
            dwMask: WTNCA_NODRAWICON,
        };
        let _ = SetWindowThemeAttribute(
            window,
            WTA_NONCLIENT,
            &options as *const _ as *const c_void,
            size_of::<WTA_OPTIONS>() as u32,
        );
        SendMessageW(
            window,
            WM_SETICON,
            Some(WPARAM(ICON_SMALL as usize)),
            Some(LPARAM(0)),
        );
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        if style & WS_EX_DLGMODALFRAME.0 as isize != 0 {
            return;
        }
        SetWindowLongPtrW(window, GWL_EXSTYLE, style | WS_EX_DLGMODALFRAME.0 as isize);
        let _ = SetWindowPos(
            window,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
    }
}

fn colorref([r, g, b]: [u8; 3]) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

pub fn set_colors(raw: isize, colors: &CaptionColors) {
    let background = colorref(colors.background);
    let foreground = colorref(colors.foreground);
    // SAFETY: COLORREF values stay live for these synchronous calls. Unsupported DWM
    // attributes (Windows 10) leave the system caption intact.
    unsafe {
        let _ = DwmSetWindowAttribute(
            handle(raw),
            DWMWA_CAPTION_COLOR,
            &background as *const _ as *const c_void,
            size_of::<COLORREF>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            handle(raw),
            DWMWA_TEXT_COLOR,
            &foreground as *const _ as *const c_void,
            size_of::<COLORREF>() as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_colors_use_bgr_byte_order() {
        assert_eq!(colorref([0x12, 0x34, 0x56]).0, 0x00563412);
    }

    #[test]
    fn hiding_the_caption_icon_preserves_window_controls_and_the_large_icon() {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GWL_STYLE, ICON_BIG, ICON_SMALL, IDI_APPLICATION,
            LoadIconW, SendMessageW, WINDOW_EX_STYLE, WM_GETICON, WM_SETICON, WS_OVERLAPPEDWINDOW,
        };
        use windows::core::w;
        // A hidden native window exercises the same style update without opening UI.
        unsafe {
            let window = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Studio caption test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                400,
                300,
                None,
                None,
                None,
                None,
            )
            .expect("a hidden test window");
            struct TestWindow(HWND);
            impl Drop for TestWindow {
                fn drop(&mut self) {
                    unsafe {
                        let _ = DestroyWindow(self.0);
                    }
                }
            }
            let _guard = TestWindow(window);
            let icon = LoadIconW(None, IDI_APPLICATION).expect("the system icon");
            for size in [ICON_SMALL, ICON_BIG] {
                SendMessageW(
                    window,
                    WM_SETICON,
                    Some(WPARAM(size as usize)),
                    Some(LPARAM(icon.0 as isize)),
                );
            }
            let style = GetWindowLongPtrW(window, GWL_STYLE);
            hide_caption_icon(window.0 as isize);
            assert_eq!(GetWindowLongPtrW(window, GWL_STYLE), style);
            assert_ne!(
                GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_DLGMODALFRAME.0 as isize,
                0
            );
            assert_eq!(
                SendMessageW(
                    window,
                    WM_GETICON,
                    Some(WPARAM(ICON_SMALL as usize)),
                    Some(LPARAM(0))
                )
                .0,
                0
            );
            assert_eq!(
                SendMessageW(
                    window,
                    WM_GETICON,
                    Some(WPARAM(ICON_BIG as usize)),
                    Some(LPARAM(0))
                )
                .0,
                icon.0 as isize
            );
        }
    }
}
