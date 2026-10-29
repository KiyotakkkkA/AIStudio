use std::sync::Arc;

use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{Manager, Runtime, State, Webview};
use zvs_contract::{BrowserBounds, BrowserCommand, BrowserState, HostEvent, parse};
use zvs_host::platform::events::{EventBus, EventSink};
use zvs_host::{IpcServer, Services};

use crate::browser::{BROWSER_UI, BrowserWorkspace};
use crate::window::STUDIO;

pub const BROWSER_MOUNT: &str = "browser.mount";
pub const BROWSER_HIDE: &str = "browser.hide";
const INVALID_BOUNDS: &str = "Invalid browser bounds";
const INVALID_COMMAND: &str = "Invalid browser command";
const NO_WORKSPACE: &str = "The browser workspace is unavailable";

pub struct Host {
    pub server: Arc<IpcServer>,
    pub events: EventBus,
    pub services: Arc<Services>,
}

struct ChannelSink(Channel<HostEvent>);

impl EventSink for ChannelSink {
    fn deliver(&self, event: &HostEvent) -> Result<(), String> {
        self.0
            .send(event.clone())
            .map_err(|error| error.to_string())
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

async fn browser_channel<R: Runtime>(
    webview: &Webview<R>,
    channel: &str,
    payload: Value,
) -> Option<Result<Value, String>> {
    if channel != BROWSER_MOUNT && channel != BROWSER_HIDE {
        return None;
    }
    let Some(workspace) = webview
        .app_handle()
        .try_state::<BrowserWorkspace>()
        .map(|state| state.inner().clone())
    else {
        return Some(Err(NO_WORKSPACE.to_owned()));
    };
    let result = if channel == BROWSER_MOUNT {
        match parse::<BrowserBounds>(payload) {
            Ok(bounds) => blocking(move || workspace.mount(bounds)).await,
            Err(_) => Err(INVALID_BOUNDS.to_owned()),
        }
    } else {
        blocking(move || {
            workspace.hide();
            Ok(())
        })
        .await
    };
    Some(result.map(|()| Value::Null))
}

#[tauri::command]
pub async fn ipc_call<R: Runtime>(
    webview: Webview<R>,
    host: State<'_, Host>,
    channel: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    ensure(webview.label(), STUDIO)?;
    let payload = payload.unwrap_or(Value::Null);
    if let Some(result) = browser_channel(&webview, &channel, payload.clone()).await {
        return result;
    }
    Ok(host.server.dispatch(&channel, payload).await)
}

#[tauri::command]
pub async fn events_subscribe<R: Runtime>(
    webview: Webview<R>,
    host: State<'_, Host>,
    channel: Channel<HostEvent>,
) -> Result<(), String> {
    ensure(webview.label(), STUDIO)?;
    host.events.attach(Arc::new(ChannelSink(channel)));
    Ok(())
}

#[tauri::command]
pub async fn browser_command<R: Runtime>(
    webview: Webview<R>,
    workspace: State<'_, BrowserWorkspace>,
    command: Value,
) -> Result<Value, String> {
    ensure(webview.label(), BROWSER_UI)?;
    let command = parse::<BrowserCommand>(command).map_err(|_| INVALID_COMMAND.to_owned())?;
    let workspace = workspace.inner().clone();
    let output = blocking(move || workspace.command(command)).await?;
    serde_json::to_value(output).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn browser_subscribe<R: Runtime>(
    webview: Webview<R>,
    workspace: State<'_, BrowserWorkspace>,
    channel: Channel<BrowserState>,
) -> Result<(), String> {
    ensure(webview.label(), BROWSER_UI)?;
    workspace.subscribe(channel);
    Ok(())
}

fn ensure(label: &str, expected: &str) -> Result<(), String> {
    if label == expected {
        Ok(())
    } else {
        Err(format!("Вебвью «{label}» не может вызывать хост"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_studio_webview_reaches_the_host() {
        assert!(ensure(STUDIO, STUDIO).is_ok());
        let refused = ensure(BROWSER_UI, STUDIO).expect_err("refused");
        assert!(refused.contains(BROWSER_UI));
        assert!(ensure("browser-site-0", STUDIO).is_err());
    }

    #[test]
    fn only_the_chrome_reaches_browser_commands() {
        assert!(ensure(BROWSER_UI, BROWSER_UI).is_ok());
        assert!(ensure(STUDIO, BROWSER_UI).is_err());
        assert!(ensure("browser-site-3", BROWSER_UI).is_err());
    }
}
