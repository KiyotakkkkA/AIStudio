use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::shell::ShellState;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{
    AppHandle, Manager, Runtime, Theme, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use zvs_host::boot::BootProgress;
use zvs_host::data::MigrationFailed;
use zvs_host::platform::window_state::{
    NormalBounds, WindowLike, WindowState, capture_window_state,
};

pub const STUDIO: &str = "studio";
pub const RECOVERY: &str = "recovery";
pub const BOOT: &str = "boot";
const RECOVERY_PAGE: &str = "recovery/index.html";
const RECOVERY_QUIT: &str = "/recovery/quit";
const BOOT_PAGE: &str = "boot/index.html";
pub const BROWSER_ONLY_PAGE: &str = "index.html#/standalone/browser";
const BOOT_REPORT_INTERVAL: Duration = Duration::from_millis(200);

static SHELL_PROFILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_shell_profile(dir: PathBuf) {
    *SHELL_PROFILE.lock().unwrap_or_else(PoisonError::into_inner) = Some(dir);
}

pub fn shell_profile() -> Option<PathBuf> {
    SHELL_PROFILE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

struct Tracked<'a, R: Runtime> {
    window: &'a WebviewWindow<R>,
    normal: NormalBounds,
}

impl<R: Runtime> WindowLike for Tracked<'_, R> {
    fn is_maximized(&self) -> bool {
        self.window.is_maximized().unwrap_or(false)
    }

    fn is_full_screen(&self) -> bool {
        self.window.is_fullscreen().unwrap_or(false)
    }

    fn normal_bounds(&self) -> NormalBounds {
        self.normal
    }
}

fn normal_bounds<R: Runtime>(window: &WebviewWindow<R>) -> Option<NormalBounds> {
    let transient = window.is_maximized().ok()?
        || window.is_minimized().ok()?
        || window.is_fullscreen().ok()?;
    if transient {
        return None;
    }
    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?.to_logical::<f64>(scale);
    let size = window.inner_size().ok()?.to_logical::<f64>(scale);
    (size.width > 0.0 && size.height > 0.0).then_some(NormalBounds {
        x: Some(position.x),
        y: Some(position.y),
        width: size.width,
        height: size.height,
    })
}

pub fn create_studio<R: Runtime>(
    app: &AppHandle<R>,
    state: WindowState,
    hidden: bool,
    page: Option<&str>,
    shell: Arc<ShellState>,
    persist: impl Fn(WindowState) + Send + Sync + 'static,
) -> tauri::Result<()> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == STUDIO)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    if let Some(page) = page {
        config.url = WebviewUrl::App(page.into());
        config.decorations = true;
    }
    let origin = app_origin(app, config.use_https_scheme);
    let shown = Arc::new(AtomicBool::new(hidden));

    let mut builder = WebviewWindowBuilder::from_config(app, &config)?
        .disable_drag_drop_handler()
        .inner_size(state.bounds.width as f64, state.bounds.height as f64)
        .maximized(state.maximized);
    if let (Some(x), Some(y)) = (state.bounds.x, state.bounds.y) {
        builder = builder.position(x as f64, y as f64);
    }
    if let Some(args) = crate::e2e::browser_args() {
        builder = builder.additional_browser_args(&args);
    }
    if let Some(dir) = shell_profile() {
        builder = builder.data_directory(dir);
    }
    let window = builder
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
    if page.is_none() {
        crate::studio_chrome::install(app);
    }

    let normal = Arc::new(Mutex::new(
        normal_bounds(&window).unwrap_or_else(|| state.bounds.into()),
    ));
    let tracked = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if matches!(event, WindowEvent::Resized(_)) {
                crate::studio_chrome::refresh(tracked.app_handle());
            }
            let window = tracked.clone();
            let normal = Arc::clone(&normal);
            tauri::async_runtime::spawn(async move {
                let sampled = window.clone();
                let _ = window.run_on_main_thread(move || {
                    if let Some(bounds) = normal_bounds(&sampled) {
                        *normal.lock().unwrap_or_else(PoisonError::into_inner) = bounds;
                    }
                });
            });
        }
        WindowEvent::ThemeChanged(_) | WindowEvent::Focused(_) => {
            crate::studio_chrome::refresh(tracked.app_handle());
        }
        WindowEvent::CloseRequested { api, .. } => {
            let normal = *normal.lock().unwrap_or_else(PoisonError::into_inner);
            persist(capture_window_state(&Tracked {
                window: &tracked,
                normal,
            }));
            if shell.keeps_running() {
                api.prevent_close();
                let _ = tracked.hide();
            }
        }
        _ => {}
    });
    Ok(())
}

pub fn recovery_url(failure: &MigrationFailed) -> String {
    let mut url = Url::parse("http://recovery.invalid/").expect("a static url");
    url.query_pairs_mut()
        .append_pair("message", &failure.message)
        .append_pair("backups", &failure.backups_dir.to_string_lossy());
    format!("{RECOVERY_PAGE}?{}", url.query().unwrap_or_default())
}

pub fn create_recovery<R: Runtime>(
    app: &AppHandle<R>,
    failure: &MigrationFailed,
) -> tauri::Result<()> {
    let origin = app_origin(app, false);
    let handle = app.clone();
    let mut builder =
        WebviewWindowBuilder::new(app, RECOVERY, WebviewUrl::App(recovery_url(failure).into()))
            .title("AI Studio Desktop — восстановление")
            .inner_size(720.0, 480.0)
            .on_navigation(move |url| {
                let allowed = origin
                    .as_ref()
                    .is_some_and(|origin| same_origin(url, origin));
                if allowed && url.path() == RECOVERY_QUIT {
                    handle.exit(0);
                    return false;
                }
                allowed
            })
            .on_new_window(|_, _| NewWindowResponse::Deny);
    if let Some(dir) = shell_profile() {
        builder = builder.data_directory(dir);
    }
    let window = builder.build()?;
    window.set_background_color(Some(canvas_color(window.theme().unwrap_or(Theme::Light))))?;
    Ok(())
}

pub fn create_boot<R: Runtime>(app: &AppHandle<R>, closable: bool) -> tauri::Result<()> {
    let origin = app_origin(app, false);
    let mut builder = WebviewWindowBuilder::new(app, BOOT, WebviewUrl::App(BOOT_PAGE.into()))
        .title("AI Studio Desktop")
        .inner_size(520.0, 220.0)
        .resizable(false)
        .maximizable(false)
        .closable(closable)
        .on_navigation(move |url| {
            origin
                .as_ref()
                .is_some_and(|origin| same_origin(url, origin))
        })
        .on_new_window(|_, _| NewWindowResponse::Deny);
    if let Some(dir) = shell_profile() {
        builder = builder.data_directory(dir);
    }
    let window = builder.build()?;
    window.set_background_color(Some(canvas_color(window.theme().unwrap_or(Theme::Light))))?;
    Ok(())
}

pub fn close_boot<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(BOOT) {
        let _ = window.destroy();
    }
}

pub fn boot_script(progress: &BootProgress) -> String {
    let payload = serde_json::json!({
        "step": progress.step,
        "steps": progress.steps,
        "label": progress.label,
        "done": progress.done,
        "total": progress.total,
    });
    format!("window.bootProgress && window.bootProgress({payload});")
}

pub struct BootReporter<R: Runtime> {
    app: AppHandle<R>,
    last: Mutex<Option<Instant>>,
}

impl<R: Runtime> BootReporter<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            last: Mutex::new(None),
        }
    }

    pub fn report(&self, progress: BootProgress) {
        let finished = progress.done >= progress.total;
        {
            let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
            let due = last.is_none_or(|at| at.elapsed() >= BOOT_REPORT_INTERVAL);
            if !due && !finished {
                return;
            }
            *last = Some(Instant::now());
        }
        if let Some(window) = self.app.get_webview_window(BOOT) {
            let _ = window.eval(boot_script(&progress));
        }
    }
}

pub fn focus_studio<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_window(STUDIO).or_else(|| app.get_window(RECOVERY)) else {
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

pub fn app_origin<R: Runtime>(app: &AppHandle<R>, use_https_scheme: bool) -> Option<Url> {
    if tauri::is_dev()
        && let Some(url) = &app.config().build.dev_url
    {
        return Some(url.clone());
    }
    let origin = if cfg!(any(windows, target_os = "android")) {
        let scheme = if use_https_scheme { "https" } else { "http" };
        format!("{scheme}://tauri.localhost")
    } else {
        "tauri://localhost".to_owned()
    };
    Url::parse(&origin).ok()
}

pub fn same_origin(url: &Url, origin: &Url) -> bool {
    url.scheme() == origin.scheme()
        && url.host_str() == origin.host_str()
        && url.port_or_known_default() == origin.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
    fn the_recovery_page_receives_the_failure_in_its_query() {
        let failure = MigrationFailed {
            message: "near \"(\": syntax error & 100% #broken".to_owned(),
            backups_dir: PathBuf::from("C:\\Users\\me\\AppData\\backups"),
        };
        let path = recovery_url(&failure);
        assert!(path.starts_with("recovery/index.html?message="));
        let parsed = url("http://tauri.localhost/").join(&path).unwrap();
        assert_eq!(parsed.path(), "/recovery/index.html");
        assert_eq!(parsed.fragment(), None);
        let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            [
                ("message".to_owned(), failure.message.clone()),
                (
                    "backups".to_owned(),
                    "C:\\Users\\me\\AppData\\backups".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn the_quit_link_resolves_to_the_intercepted_path() {
        let page = url("http://tauri.localhost/recovery/index.html?message=x");
        assert_eq!(page.join("quit").unwrap().path(), RECOVERY_QUIT);
    }

    #[test]
    fn canvas_follows_the_os_theme() {
        assert_eq!(canvas_color(Theme::Dark), Color(0x0f, 0x0d, 0x0b, 0xff));
        assert_eq!(canvas_color(Theme::Light), Color(0xec, 0xe6, 0xdd, 0xff));
    }
}
