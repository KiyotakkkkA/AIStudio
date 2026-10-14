use std::sync::Arc;

use tauri::webview::PlatformWebview;
use zvs_host::browser::is_web_url;

use crate::browser::cookies::CookieMeta;
use crate::browser::model::PageSignal;

pub type SignalSink = Arc<dyn Fn(PageSignal) + Send + Sync>;
pub type CookieReply = Box<dyn FnOnce(Result<Vec<CookieMeta>, String>)>;
pub type CookieFilter = Box<dyn Fn(&CookieMeta) -> bool + Send>;

const UNSUPPORTED: &str = "The browser workspace needs WebView2";

pub fn allowed_main(uri: &str) -> bool {
    is_web_url(uri) || uri == "about:blank"
}

pub fn strip_host_bridge(_platform: &PlatformWebview) -> Result<(), String> {
    Err(UNSUPPORTED.to_owned())
}

pub fn observe_site(_platform: &PlatformWebview, _sink: SignalSink) -> Result<(), String> {
    Err(UNSUPPORTED.to_owned())
}

pub fn round_corners(_platform: &PlatformWebview, _radius: i32) -> Result<(), String> {
    Err(UNSUPPORTED.to_owned())
}

pub fn go_back(_platform: &PlatformWebview) -> Result<(), String> {
    Err(UNSUPPORTED.to_owned())
}

pub fn go_forward(_platform: &PlatformWebview) -> Result<(), String> {
    Err(UNSUPPORTED.to_owned())
}

pub fn list_cookies(_platform: &PlatformWebview, reply: CookieReply) {
    reply(Err(UNSUPPORTED.to_owned()));
}

pub fn delete_cookies(_platform: &PlatformWebview, _filter: CookieFilter, reply: CookieReply) {
    reply(Err(UNSUPPORTED.to_owned()));
}
