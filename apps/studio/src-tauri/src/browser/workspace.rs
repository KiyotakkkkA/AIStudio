use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use tauri::ipc::Channel;
use tauri::webview::{NewWindowFeatures, NewWindowResponse, PermissionResponse, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewUrl, Window, WindowEvent,
};
use zvs_contract::{
    BrowserBounds, BrowserCommand, BrowserCommandOutput, BrowserCookie, BrowserState,
};
use zvs_host::browser::{
    BrowserLifecycle, BrowserLifecycleEvent, LifecycleSubscription, navigation_url,
};
use zvs_host::platform::ids::new_id;
use zvs_host::platform::navigation::{SharedNavigator, WorkspaceRoute};

use crate::browser::bearer::{BearerTokens, request_filters};
use crate::browser::cookies::{CookieMeta, cleared_domains};
use crate::browser::model::{
    LinkTabs, Model, NO_ACTIVE_TAB, PageSignal, Rect, TAB_LIMIT_REACHED, Tab,
};
use crate::platform::site;
use crate::window::{STUDIO, app_origin, same_origin};

pub const BROWSER_UI: &str = "browser-ui";
const CHROME_PAGE: &str = "browser-ui/index.html";
const SITE_PREFIX: &str = "browser-site-";
const BLANK: &str = "about:blank";
const UNAVAILABLE: &str = "Studio window is unavailable";
const CHROME_RADIUS: f64 = 15.0;

enum Work {
    Signal(String, PageSignal),
    Adopt(String),
    Layout,
    Closed,
}

struct Inner {
    app: AppHandle,
    profile: PathBuf,
    lifecycle: BrowserLifecycle,
    tokens: BearerTokens,
    navigator: SharedNavigator,
    links: Mutex<LinkTabs>,
    subscription: Mutex<Option<LifecycleSubscription>>,
    model: Mutex<Model>,
    chrome: Mutex<Option<Webview>>,
    creating: Mutex<()>,
    sites: Mutex<HashMap<String, Webview>>,
    shown: Mutex<Option<String>>,
    subscribers: Mutex<Vec<Channel<BrowserState>>>,
    work: Mutex<Sender<Work>>,
    labels: AtomicU64,
    hooked: AtomicBool,
}

#[derive(Clone)]
pub struct BrowserWorkspace(Arc<Inner>);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn failed(error: impl ToString) -> String {
    error.to_string()
}

fn logical(rect: Rect) -> tauri::Rect {
    tauri::Rect {
        position: LogicalPosition::new(rect.x, rect.y).into(),
        size: LogicalSize::new(rect.width, rect.height).into(),
    }
}

fn blank() -> WebviewUrl {
    WebviewUrl::External(Url::parse(BLANK).expect("a static url"))
}

impl BrowserWorkspace {
    pub fn new(
        app: AppHandle,
        profile: PathBuf,
        lifecycle: BrowserLifecycle,
        tokens: BearerTokens,
        navigator: SharedNavigator,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let workspace = Self(Arc::new(Inner {
            app,
            profile,
            lifecycle,
            tokens,
            navigator,
            links: Mutex::default(),
            subscription: Mutex::default(),
            model: Mutex::default(),
            chrome: Mutex::default(),
            creating: Mutex::default(),
            sites: Mutex::default(),
            shown: Mutex::default(),
            subscribers: Mutex::default(),
            work: Mutex::new(sender),
            labels: AtomicU64::new(0),
            hooked: AtomicBool::new(false),
        }));
        let finished = Arc::downgrade(&workspace.0);
        let subscription = workspace.0.lifecycle.subscribe(move |event| {
            if let BrowserLifecycleEvent::LinkFinished { url } = event
                && let Some(inner) = finished.upgrade()
            {
                lock(&inner.links).finished(url);
            }
        });
        *lock(&workspace.0.subscription) = Some(subscription);
        let weak = Arc::downgrade(&workspace.0);
        std::thread::Builder::new()
            .name("browser-workspace".to_owned())
            .spawn(move || run(&weak, &receiver))
            .expect("the browser workspace thread starts");
        workspace
    }

    pub fn mount(&self, bounds: BrowserBounds) -> Result<(), String> {
        let _creating = lock(&self.0.creating);
        lock(&self.0.model).mount(bounds);
        self.ensure_chrome()?;
        if lock(&self.0.model).is_empty() {
            self.open_tab(None)?;
        }
        self.publish();
        Ok(())
    }

    pub fn open_link_tab(&self, url: &str) -> Result<(), String> {
        self.0.navigator.navigate(WorkspaceRoute::Browser);
        let _creating = lock(&self.0.creating);
        self.ensure_chrome()?;
        let label = self.open_tab(Some(url))?;
        lock(&self.0.links).opened(label, url);
        Ok(())
    }

    pub fn hide(&self) {
        lock(&self.0.model).hide();
        self.layout();
    }

    pub fn subscribe(&self, channel: Channel<BrowserState>) {
        let state = lock(&self.0.model).state();
        if channel.send(state).is_ok() {
            lock(&self.0.subscribers).push(channel);
        }
    }

    pub fn command(&self, command: BrowserCommand) -> Result<BrowserCommandOutput, String> {
        match command {
            BrowserCommand::State => {}
            BrowserCommand::New { url } => {
                self.open_tab(url.as_deref())?;
            }
            BrowserCommand::Navigate { url } => {
                let url = Url::parse(&navigation_url(&url)?).map_err(failed)?;
                let webview = self.active_webview().ok_or(NO_ACTIVE_TAB)?;
                self.link_closed(webview.label());
                lock(&self.0.model).set_sites_visible(false);
                webview.navigate(url).map_err(failed)?;
            }
            BrowserCommand::Switch { id } => lock(&self.0.model).switch(&id)?,
            BrowserCommand::Close { id } => {
                let removed = lock(&self.0.model).close(&id)?;
                self.link_closed(&removed.label);
                self.discard(&removed.label);
                self.refill()?;
            }
            BrowserCommand::Reorder { id, index } => lock(&self.0.model).reorder(&id, index)?,
            BrowserCommand::Back => {
                if let Some(webview) = self.active_webview() {
                    webview
                        .with_webview(|platform| {
                            let _ = site::go_back(&platform);
                        })
                        .map_err(failed)?;
                }
            }
            BrowserCommand::Forward => {
                if let Some(webview) = self.active_webview() {
                    webview
                        .with_webview(|platform| {
                            let _ = site::go_forward(&platform);
                        })
                        .map_err(failed)?;
                }
            }
            BrowserCommand::Reload => {
                if let Some(webview) = self.active_webview() {
                    webview.reload().map_err(failed)?;
                }
            }
            BrowserCommand::Sites { visible } => lock(&self.0.model).set_sites_visible(visible),
            BrowserCommand::Cookies => return self.cookies().map(BrowserCommandOutput::Cookies),
            BrowserCommand::RemoveCookie { id } => {
                self.remove_cookies(Box::new(move |cookie| cookie.id() == id))?;
                return self.cookies().map(BrowserCommandOutput::Cookies);
            }
            BrowserCommand::ClearSite { domain } => {
                let exact = domain.clone();
                self.remove_cookies(Box::new(move |cookie| cookie.domain == exact))?;
                self.0
                    .lifecycle
                    .emit(&BrowserLifecycleEvent::CookiesCleared { domain });
                return self.cookies().map(BrowserCommandOutput::Cookies);
            }
        }
        self.publish();
        Ok(BrowserCommandOutput::State(lock(&self.0.model).state()))
    }

    fn studio(&self) -> Result<Window, String> {
        self.0
            .app
            .get_window(STUDIO)
            .ok_or_else(|| UNAVAILABLE.to_owned())
    }

    fn post(&self, work: Work) {
        let _ = lock(&self.0.work).send(work);
    }

    fn sink(&self, label: &str) -> site::SignalSink {
        let sender = lock(&self.0.work).clone();
        let label = label.to_owned();
        Arc::new(move |signal| {
            let _ = sender.send(Work::Signal(label.clone(), signal));
        })
    }

    fn active_webview(&self) -> Option<Webview> {
        let label = lock(&self.0.model).active()?.label.clone();
        lock(&self.0.sites).get(&label).cloned()
    }

    fn any_site(&self) -> Option<Webview> {
        self.active_webview()
            .or_else(|| lock(&self.0.sites).values().next().cloned())
    }

    fn hook(&self, window: &Window) {
        if self.0.hooked.swap(true, Ordering::SeqCst) {
            return;
        }
        let sender = lock(&self.0.work).clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                let _ = sender.send(Work::Layout);
            }
            WindowEvent::Destroyed => {
                let _ = sender.send(Work::Closed);
            }
            _ => {}
        });
    }

    fn ensure_chrome(&self) -> Result<(), String> {
        if lock(&self.0.chrome).is_some() {
            return Ok(());
        }
        let window = self.studio()?;
        self.hook(&window);
        let origin = app_origin(&self.0.app, false);
        let publisher = Arc::downgrade(&self.0);
        let mut builder = WebviewBuilder::new(BROWSER_UI, WebviewUrl::App(CHROME_PAGE.into()))
            .devtools(cfg!(debug_assertions))
            .on_navigation(move |url| {
                origin
                    .as_ref()
                    .is_some_and(|origin| same_origin(url, origin))
            })
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_permission_request(|_, _| PermissionResponse::Deny)
            .on_page_load(move |_, _| {
                if let Some(inner) = publisher.upgrade() {
                    BrowserWorkspace(inner).post(Work::Layout);
                }
            });
        if let Some(args) = crate::e2e::browser_args() {
            builder = builder.additional_browser_args(&args);
        }
        if let Some(dir) = crate::e2e::webview_dir() {
            builder = builder.data_directory(dir);
        }
        let chrome = window
            .add_child(
                builder,
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1.0, 1.0),
            )
            .map_err(failed)?;
        *lock(&self.0.chrome) = Some(chrome);
        Ok(())
    }

    fn next_label(&self) -> String {
        format!(
            "{SITE_PREFIX}{}",
            self.0.labels.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn site_builder(&self, label: &str) -> WebviewBuilder<tauri::Wry> {
        let opener = Arc::downgrade(&self.0);
        WebviewBuilder::new(label, blank())
            .data_directory(self.0.profile.clone())
            .devtools(cfg!(debug_assertions))
            .on_navigation(|url| site::allowed_main(url.as_str()))
            .on_download(|_, _| false)
            .on_permission_request(|_, _| PermissionResponse::Deny)
            .on_new_window(move |url, features| {
                opener
                    .upgrade()
                    .and_then(|inner| BrowserWorkspace(inner).adopt_popup(&url, &features))
                    .unwrap_or(NewWindowResponse::Deny)
            })
    }

    fn harden(&self, webview: &Webview, label: &str) -> Result<(), String> {
        let sink = self.sink(label);
        let tokens = self.0.tokens.clone();
        let sightings: site::SightingSink = Arc::new(move |sighting| {
            tokens.observe(&sighting);
        });
        webview
            .with_webview(move |platform| {
                let _ = site::strip_host_bridge(&platform);
                let _ = site::observe_site(&platform, sink);
                let _ = site::observe_bearer(&platform, &request_filters(), sightings);
            })
            .map_err(failed)
    }

    fn link_closed(&self, label: &str) {
        let url = lock(&self.0.links).closed(label);
        if let Some(url) = url {
            self.0
                .lifecycle
                .emit(&BrowserLifecycleEvent::LinkTabClosed { url });
        }
    }

    fn open_tab(&self, url: Option<&str>) -> Result<String, String> {
        let destination = match url {
            Some(url) => Some(Url::parse(&navigation_url(url)?).map_err(failed)?),
            None => None,
        };
        if !lock(&self.0.model).has_room() {
            return Err(TAB_LIMIT_REACHED.to_owned());
        }
        let window = self.studio()?;
        let label = self.next_label();
        let webview = window
            .add_child(
                self.site_builder(&label),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1.0, 1.0),
            )
            .map_err(failed)?;
        let _ = webview.hide();
        self.harden(&webview, &label)?;
        let opened = lock(&self.0.model).open(Tab::new(new_id().to_string(), label.clone()));
        if let Err(error) = opened {
            let _ = webview.close();
            return Err(error.to_owned());
        }
        lock(&self.0.sites).insert(label.clone(), webview.clone());
        if let Some(destination) = destination {
            webview.navigate(destination).map_err(failed)?;
        }
        self.publish();
        Ok(label)
    }

    fn adopt_popup(
        &self,
        url: &Url,
        features: &NewWindowFeatures,
    ) -> Option<NewWindowResponse<tauri::Wry>> {
        if !site::allowed_main(url.as_str()) || !lock(&self.0.model).has_room() {
            return None;
        }
        let label = self.next_label();
        let host = WindowBuilder::new(&self.0.app, &label)
            .visible(false)
            .build()
            .ok()?;
        let builder = self
            .site_builder(&label)
            .with_environment(features.opener().environment.clone());
        let Ok(webview) = host.add_child(
            builder,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(1.0, 1.0),
        ) else {
            let _ = host.destroy();
            return None;
        };
        let window = self
            .harden(&webview, &label)
            .ok()
            .and_then(|()| self.0.app.get_webview_window(&label));
        let opened = window.is_some()
            && lock(&self.0.model)
                .open(Tab::new(new_id().to_string(), label.clone()))
                .is_ok();
        let Some(window) = window.filter(|_| opened) else {
            let _ = webview.close();
            let _ = host.destroy();
            return None;
        };
        lock(&self.0.sites).insert(label.clone(), webview);
        self.post(Work::Adopt(label));
        Some(NewWindowResponse::Create { window })
    }

    fn adopt(&self, label: &str) {
        let Some(webview) = lock(&self.0.sites).get(label).cloned() else {
            return;
        };
        if let Ok(window) = self.studio() {
            let _ = webview.reparent(&window);
            let _ = webview.set_auto_resize(false);
            let _ = webview.hide();
        }
        if let Some(host) = self.0.app.get_window(label)
            && host.label() != STUDIO
        {
            let _ = host.destroy();
        }
        self.publish();
    }

    fn discard(&self, label: &str) {
        if lock(&self.0.shown).as_deref() == Some(label) {
            *lock(&self.0.shown) = None;
        }
        let removed = lock(&self.0.sites).remove(label);
        if let Some(webview) = removed {
            let _ = webview.close();
        }
    }

    fn refill(&self) -> Result<(), String> {
        if lock(&self.0.model).is_empty() && lock(&self.0.chrome).is_some() {
            self.open_tab(None)?;
        }
        Ok(())
    }

    fn close_label(&self, label: &str) {
        if lock(&self.0.model).forget(label).is_none() {
            return;
        }
        self.link_closed(label);
        self.discard(label);
        let _ = self.refill();
        self.publish();
    }

    fn signal(&self, label: &str, signal: PageSignal) {
        if signal == PageSignal::Crashed {
            self.link_closed(label);
        }
        if signal == PageSignal::CloseRequested {
            self.close_label(label);
        } else if lock(&self.0.model).apply(label, signal) {
            self.publish();
        }
    }

    fn layout(&self) {
        let Some(chrome) = lock(&self.0.chrome).clone() else {
            return;
        };
        let Ok(window) = self.studio() else {
            return;
        };
        let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
            return;
        };
        let size = size.to_logical::<f64>(scale);
        let layout = lock(&self.0.model).layout(size.width, size.height);
        if layout.visible {
            let _ = chrome.set_bounds(logical(layout.chrome));
            let radius = (CHROME_RADIUS * scale).round() as i32;
            let _ = chrome.with_webview(move |platform| {
                let _ = site::round_corners(&platform, radius);
            });
            let _ = chrome.show();
        } else {
            let _ = chrome.hide();
        }
        let next = layout.site.as_ref().map(|(label, _)| label.clone());
        let previous = std::mem::replace(&mut *lock(&self.0.shown), next.clone());
        let stale = previous
            .filter(|previous| Some(previous) != next.as_ref())
            .and_then(|previous| lock(&self.0.sites).get(&previous).cloned());
        if let Some(webview) = stale {
            let _ = webview.hide();
        }
        let target = layout.site.and_then(|(label, rect)| {
            lock(&self.0.sites)
                .get(&label)
                .cloned()
                .map(|webview| (webview, rect))
        });
        if let Some((webview, rect)) = target {
            let _ = webview.set_bounds(logical(rect));
            let _ = webview.show();
        }
    }

    fn publish(&self) {
        self.layout();
        let state = lock(&self.0.model).state();
        lock(&self.0.subscribers).retain(|channel| channel.send(state.clone()).is_ok());
    }

    fn closed(&self) {
        self.0
            .lifecycle
            .emit(&BrowserLifecycleEvent::WorkspaceClosed);
        lock(&self.0.links).clear();
        lock(&self.0.model).clear();
        lock(&self.0.sites).clear();
        lock(&self.0.subscribers).clear();
        *lock(&self.0.chrome) = None;
        *lock(&self.0.shown) = None;
        self.0.hooked.store(false, Ordering::SeqCst);
    }

    fn cookies(&self) -> Result<Vec<BrowserCookie>, String> {
        let webview = self.any_site().ok_or(NO_ACTIVE_TAB)?;
        let (reply, answer) = mpsc::channel();
        webview
            .with_webview(move |platform| {
                site::list_cookies(
                    &platform,
                    Box::new(move |result| {
                        let _ = reply.send(result);
                    }),
                );
            })
            .map_err(failed)?;
        let mut cookies: Vec<BrowserCookie> = answer
            .recv()
            .map_err(failed)??
            .iter()
            .map(CookieMeta::dto)
            .collect();
        cookies.sort_by(|a, b| (&a.domain, &a.name, &a.path).cmp(&(&b.domain, &b.name, &b.path)));
        Ok(cookies)
    }

    fn remove_cookies(&self, filter: site::CookieFilter) -> Result<(), String> {
        let webview = self.any_site().ok_or(NO_ACTIVE_TAB)?;
        let (reply, answer) = mpsc::channel();
        webview
            .with_webview(move |platform| {
                site::delete_cookies(
                    &platform,
                    filter,
                    Box::new(move |result| {
                        let _ = reply.send(result);
                    }),
                );
            })
            .map_err(failed)?;
        let removed = answer.recv().map_err(failed)??;
        for domain in cleared_domains(&removed) {
            self.0
                .lifecycle
                .emit(&BrowserLifecycleEvent::CookiesCleared { domain });
        }
        Ok(())
    }
}

fn run(inner: &Weak<Inner>, receiver: &Receiver<Work>) {
    while let Ok(work) = receiver.recv() {
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let workspace = BrowserWorkspace(inner);
        match work {
            Work::Signal(label, signal) => workspace.signal(&label, signal),
            Work::Adopt(label) => workspace.adopt(&label),
            Work::Layout => workspace.layout(),
            Work::Closed => workspace.closed(),
        }
    }
}
