use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::utils::config::WindowConfig;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{AppHandle, Manager, Runtime, Theme, Url, WebviewWindowBuilder};

pub const STUDIO: &str = "studio";

pub fn create_studio<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == STUDIO)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    let origin = app_origin(app, &config);
    let shown = Arc::new(AtomicBool::new(false));

    let window = WebviewWindowBuilder::from_config(app, &config)?
        .on_navigation(move |url| {
            origin
                .as_ref()
                .is_some_and(|origin| same_origin(url, origin))
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_page_load(move |window, payload| {
            if payload.event() == PageLoadEvent::Finished && !shown.swap(true, Ordering::SeqCst) {
                let _ = window.show();
            }
        })
        .build()?;
    window.set_background_color(Some(canvas_color(window.theme().unwrap_or(Theme::Light))))?;
    Ok(())
}

pub fn focus_studio<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(STUDIO) else {
        return;
    };
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn canvas_color(theme: Theme) -> Color {
    match theme {
        Theme::Dark => Color(0x0f, 0x0d, 0x0b, 0xff),
        _ => Color(0xec, 0xe6, 0xdd, 0xff),
    }
}

fn app_origin<R: Runtime>(app: &AppHandle<R>, config: &WindowConfig) -> Option<Url> {
    if tauri::is_dev()
        && let Some(url) = &app.config().build.dev_url
    {
        return Some(url.clone());
    }
    let origin = if cfg!(any(windows, target_os = "android")) {
        let scheme = if config.use_https_scheme {
            "https"
        } else {
            "http"
        };
        format!("{scheme}://tauri.localhost")
    } else {
        "tauri://localhost".to_owned()
    };
    Url::parse(&origin).ok()
}

fn same_origin(url: &Url, origin: &Url) -> bool {
    url.scheme() == origin.scheme()
        && url.host_str() == origin.host_str()
        && url.port_or_known_default() == origin.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn navigation_stays_on_the_app_origin() {
        let origin = url("http://tauri.localhost");
        assert!(same_origin(
            &url("http://tauri.localhost/index.html#/chat"),
            &origin
        ));
        assert!(!same_origin(&url("https://tauri.localhost/"), &origin));
        assert!(!same_origin(&url("http://evil.localhost/"), &origin));
        assert!(!same_origin(&url("http://tauri.localhost:8080/"), &origin));
    }

    #[test]
    fn dev_server_origin_includes_the_port() {
        let origin = url("http://localhost:5183");
        assert!(same_origin(
            &url("http://localhost:5183/browser-ui/index.html"),
            &origin
        ));
        assert!(!same_origin(&url("http://localhost:5184/"), &origin));
        assert!(!same_origin(&url("https://example.com/"), &origin));
    }

    #[test]
    fn custom_scheme_origin_compares_by_host() {
        let origin = url("tauri://localhost");
        assert!(same_origin(&url("tauri://localhost/index.html"), &origin));
        assert!(!same_origin(&url("tauri://other/"), &origin));
    }

    #[test]
    fn canvas_follows_the_os_theme() {
        assert_eq!(canvas_color(Theme::Dark), Color(0x0f, 0x0d, 0x0b, 0xff));
        assert_eq!(canvas_color(Theme::Light), Color(0xec, 0xe6, 0xdd, 0xff));
    }
}
