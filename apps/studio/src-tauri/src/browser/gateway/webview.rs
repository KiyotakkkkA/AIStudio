use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tauri::webview::{NewWindowResponse, PermissionResponse, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl, Window};
use tokio::sync::mpsc;
use url::Url;

use super::routes::SharedRoutes;
use super::{BINDING, GatewayPage, PAGE_SCRIPT, PageFactory, SharedPage, WORLD};
use crate::browser::cdp::{
    BindingCalled, CallFunctionOn, CdpClient, DEFAULT_CALL_TIMEOUT, WebviewCdp,
};
use crate::browser::model::PageSignal;
use crate::platform::site;

pub const HOST_WINDOW: &str = "account-gateway";
const PAGE_PREFIX: &str = "account-gateway-";
const BLANK: &str = "about:blank";

static LABELS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    Loaded,
    Failed,
    Broken,
}

#[derive(Debug)]
pub struct PageWatch {
    page: String,
    current: String,
    ready: bool,
    broken: bool,
}

impl PageWatch {
    pub fn new(page: &Url) -> Self {
        Self {
            page: page.as_str().to_owned(),
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
                (self.ready && snapshot.url != self.page).then_some(Watch::Broken)
            }
            PageSignal::Finished(None) if !self.ready && self.current == self.page => {
                Some(Watch::Loaded)
            }
            PageSignal::Finished(Some(_)) if !self.ready && self.current != BLANK => {
                Some(Watch::Failed)
            }
            PageSignal::Finished(_) if self.ready => Some(Watch::Broken),
            _ => None,
        };
        if outcome == Some(Watch::Broken) {
            self.broken = true;
        }
        outcome
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
    WindowBuilder::new(app, HOST_WINDOW)
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .build()
        .map_err(failed)
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
        .on_navigation(move |url| url == &allowed || url.as_str() == BLANK)
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
        webview
            .with_webview(move |platform| {
                let _ = site::strip_host_bridge(&platform);
                let _ = site::observe_site(&platform, sink);
            })
            .map_err(failed)?;
        webview.navigate(page.clone()).map_err(failed)?;
        match loaded.recv().await {
            Some(Watch::Loaded) => {}
            _ => return Err(format!("{page} did not load")),
        }

        let cdp = WebviewCdp::client(webview.clone(), self.call_timeout);
        let mut calls = cdp.binding_calls().map_err(failed)?;
        let deliver = Arc::clone(routes);
        tauri::async_runtime::spawn(async move {
            while let Some(call) = calls.next_as::<BindingCalled>().await {
                if let Ok(call) = call
                    && call.name == BINDING
                {
                    deliver.deliver(&call.payload);
                }
            }
        });
        cdp.runtime_enable().await.map_err(failed)?;
        cdp.add_binding(BINDING, Some(WORLD))
            .await
            .map_err(failed)?;
        let frame = cdp.main_frame().await.map_err(failed)?;
        let context = cdp
            .create_isolated_world(&frame, WORLD)
            .await
            .map_err(failed)?;
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
        if href != Some(page.as_str()) {
            cdp.close();
            return Err(format!("the gateway page is at {href:?}"));
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
        let result = self
            .cdp
            .call_function_on(CallFunctionOn::new(
                *PAGE_SCRIPT,
                self.context,
                vec![command],
            ))
            .await
            .map_err(failed)?;
        Ok(result.value.unwrap_or(Value::Null))
    }

    fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst) && !self.cdp.is_closed()
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
            Some(Watch::Failed)
        );
        let mut moved = PageWatch::new(&page());
        moved.apply(&at("https://chat.qwen.ai/login"));
        assert_eq!(moved.apply(&PageSignal::Finished(None)), None);
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
