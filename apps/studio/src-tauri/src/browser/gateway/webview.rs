use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tauri::webview::{NewWindowResponse, PermissionResponse, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl, Window, WindowEvent,
};
use tokio::sync::{mpsc, oneshot};
use url::Url;

use super::routes::SharedRoutes;
use super::{BINDING, GatewayPage, PAGE_SCRIPT, PageFactory, SharedPage, WORLD};
use crate::browser::cdp::{
    BindingCalled, CallFunctionOn, CdpClient, DEFAULT_CALL_TIMEOUT, WebviewCdp,
};
use crate::browser::model::{LoadFailure, PageSignal};
use crate::platform::site;

pub const HOST_WINDOW: &str = "account-gateway";
const PAGE_PREFIX: &str = "account-gateway-";
const BLANK: &str = "about:blank";
const VERIFY_TITLE: &str = "Проверка аккаунта";
const VERIFY_SIZE: (f64, f64) = (480.0, 720.0);

static LABELS: AtomicU64 = AtomicU64::new(0);

// Qwen is a SPA: same-origin history changes keep the execution context alive.
// Other providers and test fixtures keep the static-page policy.
fn application_page(page: &Url) -> bool {
    page.as_str() == "https://chat.qwen.ai/"
}

fn allowed_location(page: &Url, location: &str) -> bool {
    if application_page(page) {
        Url::parse(location).is_ok_and(|url| url.origin() == page.origin())
    } else {
        location == page.as_str()
    }
}

fn main_context(event: &Value, frame: &str) -> Option<i64> {
    let context = event.get("context")?;
    let auxiliary = context.get("auxData")?;
    (auxiliary.get("isDefault")?.as_bool()? && auxiliary.get("frameId")?.as_str()? == frame)
        .then(|| context.get("id")?.as_i64())?
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    Loaded,
    Failed(LoadFailure),
    Broken,
}

#[derive(Debug)]
pub struct PageWatch {
    page: Url,
    current: String,
    ready: bool,
    broken: bool,
}

impl PageWatch {
    pub fn new(page: &Url) -> Self {
        Self {
            page: page.clone(),
            current: String::new(),
            ready: false,
            broken: false,
        }
    }

    pub const fn mark_ready(&mut self) {
        self.ready = true;
    }

    pub fn apply(&mut self, signal: &PageSignal) -> Option<Watch> {
        if self.broken {
            return None;
        }
        let outcome = match signal {
            PageSignal::Crashed | PageSignal::CloseRequested => Some(Watch::Broken),
            PageSignal::Started if self.ready => Some(Watch::Broken),
            PageSignal::Snapshot(snapshot) => {
                self.current.clone_from(&snapshot.url);
                (self.ready && !allowed_location(&self.page, &snapshot.url))
                    .then_some(Watch::Broken)
            }
            PageSignal::Finished(None)
                if !self.ready && allowed_location(&self.page, &self.current) =>
            {
                Some(Watch::Loaded)
            }
            PageSignal::Finished(Some(failure)) if !self.ready && self.current != BLANK => {
                Some(Watch::Failed(*failure))
            }
            PageSignal::Finished(_) if self.ready => Some(Watch::Broken),
            _ => None,
        };
        if outcome == Some(Watch::Broken) {
            self.broken = true;
        }
        outcome
    }

    fn current_location(&self) -> String {
        Url::parse(&self.current)
            .map(|url| format!("{}{}", url.origin().ascii_serialization(), url.path()))
            .unwrap_or_else(|_| "unknown".to_owned())
    }
}

pub struct WebviewPages {
    app: AppHandle,
    profile: PathBuf,
    agent: Arc<Mutex<Option<String>>>,
    call_timeout: Duration,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn failed(error: impl ToString) -> String {
    error.to_string()
}

fn host_window(app: &AppHandle) -> Result<Window, String> {
    if let Some(window) = app.get_window(HOST_WINDOW) {
        return Ok(window);
    }
    let window = WindowBuilder::new(app, HOST_WINDOW)
        .title(VERIFY_TITLE)
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .build()
        .map_err(failed)?;
    // Closing a shown verification only hides it: the pages it hosts stay alive.
    let hidden = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hidden.hide();
            let _ = hidden.set_skip_taskbar(true);
        }
    });
    Ok(window)
}

impl WebviewPages {
    pub fn new(app: AppHandle, profile: PathBuf) -> Self {
        Self {
            app,
            profile,
            agent: Arc::default(),
            call_timeout: DEFAULT_CALL_TIMEOUT,
        }
    }

    fn builder(&self, label: &str, page: &Url) -> WebviewBuilder<tauri::Wry> {
        let allowed = page.clone();
        WebviewBuilder::new(
            label,
            WebviewUrl::External(Url::parse(BLANK).expect("a static url")),
        )
        .data_directory(self.profile.clone())
        .devtools(false)
        .on_navigation(move |url| allowed_location(&allowed, url.as_str()) || url.as_str() == BLANK)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_permission_request(|_, _| PermissionResponse::Deny)
    }

    async fn create(&self, page: &Url) -> Result<Webview, String> {
        let label = format!("{PAGE_PREFIX}{}", LABELS.fetch_add(1, Ordering::Relaxed));
        let builder = self.builder(&label, page);
        let app = self.app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            host_window(&app)?
                .add_child(
                    builder,
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(1.0, 1.0),
                )
                .map_err(failed)
        })
        .await
        .map_err(failed)?
    }

    async fn prepare(
        &self,
        webview: &Webview,
        page: &Url,
        routes: &SharedRoutes,
        healthy: &Arc<AtomicBool>,
    ) -> Result<(CdpClient, i64), String> {
        let watch = Arc::new(Mutex::new(PageWatch::new(page)));
        let (sender, mut loaded) = mpsc::unbounded_channel();
        let sink: site::SignalSink = {
            let (watch, routes, healthy) =
                (Arc::clone(&watch), Arc::clone(routes), Arc::clone(healthy));
            Arc::new(move |signal| {
                let outcome = lock(&watch).apply(&signal);
                if outcome == Some(Watch::Broken) {
                    healthy.store(false, Ordering::SeqCst);
                    routes.fail_all();
                }
                if let Some(outcome) = outcome {
                    let _ = sender.send(outcome);
                }
            })
        };
        let (configured, configuration) = oneshot::channel();
        webview
            .with_webview(move |platform| {
                let result = site::strip_host_bridge(&platform)
                    .map_err(failed)
                    .and_then(|()| site::observe_site(&platform, sink).map_err(failed));
                let _ = configured.send(result);
            })
            .map_err(failed)?;
        // Do not load vendor scripts until disabling the host bridge has succeeded.
        configuration.await.map_err(failed)??;
        webview.navigate(page.clone()).map_err(failed)?;
        match loaded.recv().await {
            Some(Watch::Loaded) => {}
            Some(Watch::Failed(failure)) => {
                let location = lock(&watch).current_location();
                return Err(format!(
                    "The gateway page failed to load ({failure:?}) at {location}"
                ));
            }
            Some(Watch::Broken) => {
                return Err("The gateway webview stopped while loading".to_owned());
            }
            None => return Err("The gateway page load observer closed".to_owned()),
        }

        let cdp = WebviewCdp::client(webview.clone(), self.call_timeout);
        let mut calls = cdp.binding_calls().map_err(failed)?;
        // Subscribe before enabling Runtime: existing contexts are announced by enable.
        let mut contexts = cdp
            .subscribe("Runtime.executionContextCreated")
            .map_err(failed)?;
        let frame = cdp.main_frame().await.map_err(failed)?;
        cdp.runtime_enable().await.map_err(failed)?;
        let context = if application_page(page) {
            let context = tokio::time::timeout(self.call_timeout, async {
                while let Some(event) = contexts.next().await {
                    if let Some(context) = main_context(&event, &frame) {
                        return Ok(context);
                    }
                }
                Err("the gateway main context is missing".to_owned())
            })
            .await
            .map_err(failed)??;
            // This binding accepts response messages only; never expose the Tauri bridge.
            cdp.send(
                "Runtime.addBinding",
                json!({
                    "name": BINDING, "executionContextId": context
                }),
            )
            .await
            .map_err(failed)?;
            context
        } else {
            cdp.add_binding(BINDING, Some(WORLD))
                .await
                .map_err(failed)?;
            cdp.create_isolated_world(&frame, WORLD)
                .await
                .map_err(failed)?
        };
        let deliver = Arc::clone(routes);
        tauri::async_runtime::spawn(async move {
            while let Some(call) = calls.next_as::<BindingCalled>().await {
                if let Ok(call) = call
                    && call.name == BINDING
                    && call.execution_context_id == context
                {
                    deliver.deliver(&call.payload);
                }
            }
        });
        let probe = cdp
            .call_function_on(CallFunctionOn::new(
                *PAGE_SCRIPT,
                context,
                vec![json!({ "op": "probe" })],
            ))
            .await
            .map_err(failed)?;
        let href = probe
            .value
            .as_ref()
            .and_then(|value| value.get("href"))
            .and_then(Value::as_str);
        if !href.is_some_and(|href| allowed_location(page, href)) {
            cdp.close();
            return Err(format!("the gateway page is at {href:?}"));
        }
        if probe
            .value
            .as_ref()
            .and_then(|value| value.get("readyState"))
            .and_then(Value::as_str)
            != Some("complete")
        {
            cdp.close();
            return Err("the gateway document has not finished loading".to_owned());
        }
        lock(&watch).mark_ready();
        self.remember_agent(webview);
        Ok((cdp, context))
    }

    fn remember_agent(&self, webview: &Webview) {
        if lock(&self.agent).is_some() {
            return;
        }
        let agent = Arc::clone(&self.agent);
        let _ = webview.with_webview(move |platform| {
            if let Ok(value) = site::user_agent(&platform)
                && !value.is_empty()
            {
                *lock(&agent) = Some(value);
            }
        });
    }
}

#[async_trait]
impl PageFactory for WebviewPages {
    async fn open(
        &self,
        _origin: &str,
        page: &Url,
        routes: SharedRoutes,
    ) -> Result<SharedPage, String> {
        let webview = self.create(page).await?;
        let healthy = Arc::new(AtomicBool::new(true));
        match self.prepare(&webview, page, &routes, &healthy).await {
            Ok((cdp, context)) => Ok(Arc::new(WebviewPage {
                webview,
                cdp,
                context,
                healthy,
                routes,
            })),
            Err(error) => {
                routes.fail_all();
                let _ = webview.close();
                Err(error)
            }
        }
    }

    fn user_agent(&self) -> Option<String> {
        lock(&self.agent).clone()
    }

    fn dispose(&self) {
        if let Some(window) = self.app.get_window(HOST_WINDOW) {
            let _ = window.destroy();
        }
    }
}

struct WebviewPage {
    webview: Webview,
    cdp: CdpClient,
    context: i64,
    healthy: Arc<AtomicBool>,
    routes: SharedRoutes,
}

#[async_trait]
impl GatewayPage for WebviewPage {
    async fn command(&self, command: Value) -> Result<Value, String> {
        if !self.healthy() {
            return Err("the gateway page is closed".to_owned());
        }
        let awaited = command["op"] == "renew";
        let call = CallFunctionOn::new(*PAGE_SCRIPT, self.context, vec![command]);
        let result = self
            .cdp
            .call_function_on(if awaited { call.awaiting() } else { call })
            .await
            .map_err(failed)?;
        Ok(result.value.unwrap_or(Value::Null))
    }

    fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst) && !self.cdp.is_closed()
    }

    fn reveal(&self) {
        let window = self.webview.window();
        let size = LogicalSize::new(VERIFY_SIZE.0, VERIFY_SIZE.1);
        let _ = window.set_size(size);
        let _ = self.webview.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = self.webview.set_size(size);
        let _ = window.center();
        let _ = window.set_skip_taskbar(false);
        let _ = window.show();
        let _ = window.set_focus();
    }

    fn conceal(&self) {
        let window = self.webview.window();
        let _ = window.hide();
        let _ = window.set_skip_taskbar(true);
        let _ = self.webview.set_size(LogicalSize::new(1.0, 1.0));
    }

    fn close(&self) {
        self.healthy.store(false, Ordering::SeqCst);
        self.routes.fail_all();
        self.cdp.close();
        let _ = self.webview.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::model::{LoadFailure, PageSnapshot};

    fn at(url: &str) -> PageSignal {
        PageSignal::Snapshot(PageSnapshot {
            url: url.to_owned(),
            title: String::new(),
            can_go_back: false,
            can_go_forward: false,
        })
    }

    fn page() -> Url {
        Url::parse("https://chat.qwen.ai/robots.txt").expect("a url")
    }

    #[test]
    fn qwen_uses_the_app_but_fixtures_keep_the_static_page() {
        assert_eq!(
            super::super::page_url("https://chat.qwen.ai")
                .unwrap()
                .as_str(),
            "https://chat.qwen.ai/"
        );
        assert_eq!(
            super::super::page_url("http://127.0.0.1:1234")
                .unwrap()
                .path(),
            "/robots.txt"
        );
    }

    #[test]
    fn app_history_changes_are_allowed_but_reloads_and_other_origins_break_it() {
        let page = Url::parse("https://chat.qwen.ai/").unwrap();
        let mut watch = PageWatch::new(&page);
        watch.apply(&at("https://chat.qwen.ai/c/123"));
        assert_eq!(
            watch.apply(&PageSignal::Finished(None)),
            Some(Watch::Loaded)
        );
        watch.mark_ready();
        assert_eq!(watch.apply(&at("https://chat.qwen.ai/c/456")), None);
        assert_eq!(watch.apply(&PageSignal::Started), Some(Watch::Broken));
        assert!(!allowed_location(&page, "https://chat.qwen.ai.evil.test/"));
        assert!(!allowed_location(&page, "http://chat.qwen.ai/"));
        assert!(!allowed_location(&page, "https://other.example/"));
    }

    #[test]
    fn only_the_main_frames_default_context_is_selected() {
        let event = |frame: &str, default: bool| {
            json!({"context": {
                "id": 7, "auxData": {"frameId": frame, "isDefault": default}
            }})
        };
        assert_eq!(main_context(&event("main", true), "main"), Some(7));
        assert_eq!(main_context(&event("child", true), "main"), None);
        assert_eq!(main_context(&event("main", false), "main"), None);
        assert_eq!(main_context(&json!({}), "main"), None);
    }

    #[test]
    fn the_page_is_loaded_once_it_finishes_at_the_static_url() {
        let mut watch = PageWatch::new(&page());
        assert_eq!(watch.apply(&at(BLANK)), None);
        assert_eq!(watch.apply(&PageSignal::Finished(None)), None);
        assert_eq!(watch.apply(&PageSignal::Started), None);
        assert_eq!(watch.apply(&at("https://chat.qwen.ai/robots.txt")), None);
        assert_eq!(
            watch.apply(&PageSignal::Finished(None)),
            Some(Watch::Loaded)
        );
    }

    #[test]
    fn a_failed_load_or_a_redirect_is_not_ready() {
        let mut failed = PageWatch::new(&page());
        failed.apply(&at("https://chat.qwen.ai/robots.txt"));
        assert_eq!(
            failed.apply(&PageSignal::Finished(Some(LoadFailure::Other(3)))),
            Some(Watch::Failed(LoadFailure::Other(3)))
        );
        let mut moved = PageWatch::new(&page());
        moved.apply(&at("https://chat.qwen.ai/login"));
        assert_eq!(moved.apply(&PageSignal::Finished(None)), None);
    }

    #[test]
    fn a_failed_load_reports_its_code_without_logging_url_queries() {
        let mut failed = PageWatch::new(&page());
        failed.apply(&at("https://chat.qwen.ai/login?token=private"));
        assert_eq!(
            failed.apply(&PageSignal::Finished(Some(LoadFailure::Other(-105)))),
            Some(Watch::Failed(LoadFailure::Other(-105)))
        );
        assert_eq!(failed.current_location(), "https://chat.qwen.ai/login");
    }

    #[test]
    fn once_ready_any_navigation_crash_or_close_breaks_the_page() {
        for signal in [
            PageSignal::Started,
            PageSignal::Crashed,
            PageSignal::CloseRequested,
            at("https://chat.qwen.ai/"),
            PageSignal::Finished(None),
        ] {
            let mut watch = PageWatch::new(&page());
            watch.apply(&at("https://chat.qwen.ai/robots.txt"));
            watch.apply(&PageSignal::Finished(None));
            watch.mark_ready();
            assert_eq!(watch.apply(&PageSignal::Blocked), None);
            assert_eq!(watch.apply(&at("https://chat.qwen.ai/robots.txt")), None);
            assert_eq!(watch.apply(&signal), Some(Watch::Broken), "{signal:?}");
            assert_eq!(watch.apply(&PageSignal::Crashed), None);
        }
        let mut loading = PageWatch::new(&page());
        assert_eq!(loading.apply(&PageSignal::Crashed), Some(Watch::Broken));
    }
}
