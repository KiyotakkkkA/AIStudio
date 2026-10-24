#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ::windows::Win32::Foundation::{HWND, RECT};
use ::windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
use ::windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use ::windows::core::{BOOL, Error, HSTRING, Interface, PCWSTR, PWSTR, w};
use tauri::webview::PlatformWebview;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_NAVIGATION_KIND, COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD,
    COREWEBVIEW2_WEB_ERROR_STATUS, COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED,
    COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT, ICoreWebView2, ICoreWebView2_2,
    ICoreWebView2_22, ICoreWebView2Cookie, ICoreWebView2CookieList,
    ICoreWebView2NavigationStartingEventArgs, ICoreWebView2NavigationStartingEventArgs3,
    ICoreWebView2Settings2,
};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, DOMContentLoadedEventHandler,
    DevToolsProtocolEventReceivedEventHandler, DocumentTitleChangedEventHandler,
    GetCookiesCompletedHandler, HistoryChangedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, ProcessFailedEventHandler, SourceChangedEventHandler,
    WebResourceRequestedEventHandler, WindowCloseRequestedEventHandler, take_pwstr,
};
use zvs_host::browser::is_web_url;

use crate::browser::bearer::Sighting;
use crate::browser::cdp::{CdpEventSink, CdpReply};
use crate::browser::cookies::CookieMeta;
use crate::browser::model::{LoadFailure, PageSignal, PageSnapshot};

pub type SignalSink = Arc<dyn Fn(PageSignal) + Send + Sync>;
pub type SightingSink = Arc<dyn Fn(Sighting) + Send + Sync>;
pub type CookieReply = Box<dyn FnOnce(Result<Vec<CookieMeta>, String>)>;
pub type CookieFilter = Box<dyn Fn(&CookieMeta) -> bool + Send>;

const HOST_SCRIPT_IDS: u32 = 64;
const BLANK: &str = "about:blank";
const SRCDOC: &str = "about:srcdoc";

fn core(platform: &PlatformWebview) -> ::windows::core::Result<ICoreWebView2> {
    unsafe { platform.controller().CoreWebView2() }
}

fn string(read: impl FnOnce(*mut PWSTR) -> ::windows::core::Result<()>) -> String {
    let mut value = PWSTR::null();
    if read(&raw mut value).is_err() {
        return String::new();
    }
    take_pwstr(value)
}

fn flag(read: impl FnOnce(*mut BOOL) -> ::windows::core::Result<()>) -> bool {
    let mut value = BOOL::default();
    read(&raw mut value).is_ok() && value.as_bool()
}

fn snapshot(webview: &ICoreWebView2) -> PageSnapshot {
    unsafe {
        PageSnapshot {
            url: string(|value| webview.Source(value)),
            title: string(|value| webview.DocumentTitle(value)),
            can_go_back: flag(|value| webview.CanGoBack(value)),
            can_go_forward: flag(|value| webview.CanGoForward(value)),
        }
    }
}

fn starting_uri(args: &ICoreWebView2NavigationStartingEventArgs) -> String {
    unsafe { string(|value| args.Uri(value)) }
}

fn is_history(args: &ICoreWebView2NavigationStartingEventArgs) -> bool {
    let Ok(args) = args.cast::<ICoreWebView2NavigationStartingEventArgs3>() else {
        return false;
    };
    let mut kind = COREWEBVIEW2_NAVIGATION_KIND::default();
    unsafe { args.NavigationKind(&raw mut kind) }.is_ok()
        && kind == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD
}

pub fn failure(status: COREWEBVIEW2_WEB_ERROR_STATUS, history: bool) -> LoadFailure {
    if status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
        LoadFailure::Aborted
    } else if status == COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN && history {
        LoadFailure::CacheMiss
    } else {
        LoadFailure::Other(status.0)
    }
}

pub fn allowed_main(uri: &str) -> bool {
    is_web_url(uri) || uri == BLANK
}

pub fn allowed_frame(uri: &str) -> bool {
    is_web_url(uri) || uri == BLANK || uri == SRCDOC
}

pub fn strip_host_bridge(platform: &PlatformWebview) -> ::windows::core::Result<()> {
    let webview = core(platform)?;
    for id in 1..=HOST_SCRIPT_IDS {
        let _ = unsafe {
            webview.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id.to_string()))
        };
    }
    unsafe { webview.Settings()?.SetIsWebMessageEnabled(false) }
}

pub fn observe_site(platform: &PlatformWebview, sink: SignalSink) -> ::windows::core::Result<()> {
    let webview = core(platform)?;
    let history = Arc::new(AtomicBool::new(false));
    let mut token = 0_i64;
    unsafe {
        let (signal, started) = (Arc::clone(&sink), Arc::clone(&history));
        webview.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let cancelled = flag(|value| args.Cancel(value));
                if cancelled || !allowed_main(&starting_uri(&args)) {
                    args.SetCancel(true)?;
                    signal(PageSignal::Blocked);
                    return Ok(());
                }
                started.store(is_history(&args), Ordering::SeqCst);
                signal(PageSignal::Started);
                Ok(())
            })),
            &raw mut token,
        )?;

        let signal = Arc::clone(&sink);
        webview.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                if !allowed_frame(&starting_uri(&args)) {
                    args.SetCancel(true)?;
                    signal(PageSignal::Blocked);
                }
                Ok(())
            })),
            &raw mut token,
        )?;

        let (signal, finished) = (Arc::clone(&sink), Arc::clone(&history));
        webview.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                let Some(args) = args else { return Ok(()) };
                let failed = if flag(|value| args.IsSuccess(value)) {
                    None
                } else {
                    let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                    args.WebErrorStatus(&raw mut status)?;
                    Some(failure(status, finished.load(Ordering::SeqCst)))
                };
                if let Some(sender) = sender {
                    signal(PageSignal::Snapshot(snapshot(&sender)));
                }
                signal(PageSignal::Finished(failed));
                Ok(())
            })),
            &raw mut token,
        )?;

        let signal = Arc::clone(&sink);
        webview.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(sender) = sender {
                    signal(PageSignal::Snapshot(snapshot(&sender)));
                }
                Ok(())
            })),
            &raw mut token,
        )?;

        let signal = Arc::clone(&sink);
        webview.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(sender) = sender {
                    signal(PageSignal::Snapshot(snapshot(&sender)));
                }
                Ok(())
            })),
            &raw mut token,
        )?;

        let signal = Arc::clone(&sink);
        webview.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(sender) = sender {
                    signal(PageSignal::Snapshot(snapshot(&sender)));
                }
                Ok(())
            })),
            &raw mut token,
        )?;

        let signal = Arc::clone(&sink);
        webview.cast::<ICoreWebView2_2>()?.add_DOMContentLoaded(
            &DOMContentLoadedEventHandler::create(Box::new(move |_, _| {
                signal(PageSignal::Interactive);
                Ok(())
            })),
            &raw mut token,
        )?;

        let signal = Arc::clone(&sink);
        webview.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, _| {
                signal(PageSignal::Crashed);
                Ok(())
            })),
            &raw mut token,
        )?;

        webview.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                sink(PageSignal::CloseRequested);
                Ok(())
            })),
            &raw mut token,
        )?;
    }
    Ok(())
}

pub fn round_corners(platform: &PlatformWebview, radius: i32) -> ::windows::core::Result<()> {
    let controller = platform.controller();
    let mut host = HWND::default();
    let mut client = RECT::default();
    unsafe {
        controller.ParentWindow(&raw mut host)?;
        GetClientRect(host, &raw mut client)?;
        let diameter = radius.saturating_mul(2);
        let region = CreateRoundRectRgn(
            0,
            0,
            client.right + 1,
            client.bottom + 1,
            diameter,
            diameter,
        );
        if region.is_invalid() {
            return Err(Error::from_thread());
        }
        if SetWindowRgn(host, Some(region), true) == 0 {
            let error = Error::from_thread();
            let _ = DeleteObject(region.into());
            return Err(error);
        }
    }
    Ok(())
}

pub fn go_back(platform: &PlatformWebview) -> ::windows::core::Result<()> {
    let webview = core(platform)?;
    unsafe {
        if flag(|value| webview.CanGoBack(value)) {
            webview.GoBack()?;
        }
    }
    Ok(())
}

pub fn go_forward(platform: &PlatformWebview) -> ::windows::core::Result<()> {
    let webview = core(platform)?;
    unsafe {
        if flag(|value| webview.CanGoForward(value)) {
            webview.GoForward()?;
        }
    }
    Ok(())
}

fn cookie_meta(cookie: &ICoreWebView2Cookie) -> CookieMeta {
    unsafe {
        let mut expires = -1.0;
        let _ = cookie.Expires(&raw mut expires);
        CookieMeta {
            domain: string(|value| cookie.Domain(value)),
            name: string(|value| cookie.Name(value)),
            path: string(|value| cookie.Path(value)),
            secure: flag(|value| cookie.IsSecure(value)),
            http_only: flag(|value| cookie.IsHttpOnly(value)),
            session: flag(|value| cookie.IsSession(value)),
            expires,
        }
    }
}

fn cookie_list(list: Option<&ICoreWebView2CookieList>) -> Vec<(CookieMeta, ICoreWebView2Cookie)> {
    let Some(list) = list else {
        return Vec::new();
    };
    let mut count = 0_u32;
    if unsafe { list.Count(&raw mut count) }.is_err() {
        return Vec::new();
    }
    (0..count)
        .filter_map(|index| unsafe { list.GetValueAtIndex(index) }.ok())
        .map(|cookie| (cookie_meta(&cookie), cookie))
        .collect()
}

fn read_cookies(
    platform: &PlatformWebview,
    then: impl FnOnce(::windows::core::Result<Vec<(CookieMeta, ICoreWebView2Cookie)>>) + 'static,
) -> ::windows::core::Result<()> {
    let manager = unsafe { core(platform)?.cast::<ICoreWebView2_2>()?.CookieManager()? };
    let handler = GetCookiesCompletedHandler::create(Box::new(move |result, list| {
        then(result.map(|()| cookie_list(list.as_ref())));
        Ok(())
    }));
    unsafe { manager.GetCookies(PCWSTR::null(), &handler) }
}

pub fn list_cookies(platform: &PlatformWebview, reply: CookieReply) {
    let reply = Rc::new(RefCell::new(Some(reply)));
    let answer = Rc::clone(&reply);
    let started = read_cookies(platform, move |result| {
        if let Some(reply) = answer.borrow_mut().take() {
            reply(
                result
                    .map(|cookies| cookies.into_iter().map(|(meta, _)| meta).collect())
                    .map_err(|error| error.to_string()),
            );
        }
    });
    if let Err(error) = started
        && let Some(reply) = reply.borrow_mut().take()
    {
        reply(Err(error.to_string()));
    }
}

pub fn delete_cookies(platform: &PlatformWebview, filter: CookieFilter, reply: CookieReply) {
    let manager = match core(platform)
        .and_then(|webview| webview.cast::<ICoreWebView2_2>())
        .and_then(|webview| unsafe { webview.CookieManager() })
    {
        Ok(manager) => manager,
        Err(error) => return reply(Err(error.to_string())),
    };
    let reply = Rc::new(RefCell::new(Some(reply)));
    let answer = Rc::clone(&reply);
    let started = read_cookies(platform, move |result| {
        let outcome = result.map_err(|error| error.to_string()).map(|cookies| {
            cookies
                .into_iter()
                .filter(|(meta, _)| filter(meta))
                .filter(|(_, cookie)| unsafe { manager.DeleteCookie(cookie) }.is_ok())
                .map(|(meta, _)| meta)
                .collect()
        });
        if let Some(reply) = answer.borrow_mut().take() {
            reply(outcome);
        }
    });
    if let Err(error) = started
        && let Some(reply) = reply.borrow_mut().take()
    {
        reply(Err(error.to_string()));
    }
}

pub fn call_devtools(platform: &PlatformWebview, method: &str, params: &str, reply: CdpReply) {
    let reply = Rc::new(RefCell::new(Some(reply)));
    let answer = Rc::clone(&reply);
    let handler =
        CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
            if let Some(reply) = answer.borrow_mut().take() {
                reply(match result {
                    Ok(()) => Ok(json),
                    Err(error) if json.is_empty() => Err(error.to_string()),
                    Err(_) => Err(json),
                });
            }
            Ok(())
        }));
    let started = core(platform).and_then(|webview| unsafe {
        webview.CallDevToolsProtocolMethod(&HSTRING::from(method), &HSTRING::from(params), &handler)
    });
    if let Err(error) = started
        && let Some(reply) = reply.borrow_mut().take()
    {
        reply(Err(error.to_string()));
    }
}

pub fn listen_devtools(
    platform: &PlatformWebview,
    event: &str,
    sink: CdpEventSink,
) -> ::windows::core::Result<()> {
    let webview = core(platform)?;
    let mut token = 0_i64;
    unsafe {
        let receiver = webview.GetDevToolsProtocolEventReceiver(&HSTRING::from(event))?;
        receiver.add_DevToolsProtocolEventReceived(
            &DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    sink(string(|value| args.ParameterObjectAsJson(value)));
                }
                Ok(())
            })),
            &raw mut token,
        )
    }
}

pub fn observe_bearer(
    platform: &PlatformWebview,
    origins: &[String],
    sink: SightingSink,
) -> ::windows::core::Result<()> {
    let webview = core(platform)?;
    let scoped = webview.cast::<ICoreWebView2_22>().ok();
    let mut token = 0_i64;
    unsafe {
        for origin in origins {
            let filter = HSTRING::from(format!("{origin}/*"));
            match &scoped {
                Some(scoped) => scoped.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &filter,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT,
                )?,
                None => {
                    webview.AddWebResourceRequestedFilter(
                        &filter,
                        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    )?;
                }
            }
        }
        webview.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
                let (Some(sender), Some(args)) = (sender, args) else {
                    return Ok(());
                };
                let request = args.Request()?;
                let headers = request.Headers()?;
                let authorization = if flag(|value| headers.Contains(w!("Authorization"), value)) {
                    Some(string(|value| {
                        headers.GetHeader(w!("Authorization"), value)
                    }))
                } else {
                    None
                };
                sink(Sighting {
                    request_url: string(|value| request.Uri(value)),
                    page_url: string(|value| sender.Source(value)),
                    authorization,
                });
                Ok(())
            })),
            &raw mut token,
        )
    }
}

pub fn user_agent(platform: &PlatformWebview) -> ::windows::core::Result<String> {
    let settings = unsafe {
        core(platform)?
            .Settings()?
            .cast::<ICoreWebView2Settings2>()?
    };
    let agent = unsafe { string(|value| settings.UserAgent(value)) };
    Ok(agent)
}

#[cfg(test)]
mod tests {
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED;

    use super::*;

    #[test]
    fn only_web_pages_load_in_the_main_frame() {
        for allowed in ["https://example.com/", "http://127.0.0.1:8080/", BLANK] {
            assert!(allowed_main(allowed), "{allowed}");
        }
        for refused in [
            "file:///C:/",
            "javascript:alert(1)",
            "data:text/html,x",
            SRCDOC,
            "http://user:pass@example.com/",
            "tauri://localhost/",
        ] {
            assert!(!allowed_main(refused), "{refused}");
        }
    }

    #[test]
    fn frames_may_also_be_srcdoc() {
        assert!(allowed_frame(SRCDOC));
        assert!(allowed_frame(BLANK));
        assert!(!allowed_frame("data:text/html,x"));
        assert!(!allowed_frame("file:///C:/"));
    }

    #[test]
    fn web_error_statuses_map_to_load_failures() {
        assert_eq!(
            failure(COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, false),
            LoadFailure::Aborted
        );
        assert_eq!(
            failure(COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, true),
            LoadFailure::CacheMiss
        );
        assert_eq!(
            failure(COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, false),
            LoadFailure::Other(COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN.0)
        );
        assert_eq!(
            failure(COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED, false),
            LoadFailure::Other(COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED.0)
        );
    }
}
