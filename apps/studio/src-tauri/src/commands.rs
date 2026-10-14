use std::sync::Arc;

use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{Runtime, State, Webview};
use zvs_contract::HostEvent;
use zvs_host::platform::events::{EventBus, EventSink};
use zvs_host::{IpcServer, Services};

use crate::window::STUDIO;

pub struct Host {
    pub server: IpcServer,
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

#[tauri::command]
pub async fn ipc_call<R: Runtime>(
    webview: Webview<R>,
    host: State<'_, Host>,
    channel: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    ensure_studio(webview.label())?;
    Ok(host
        .server
        .dispatch(&channel, payload.unwrap_or(Value::Null))
        .await)
}

#[tauri::command]
pub async fn events_subscribe<R: Runtime>(
    webview: Webview<R>,
    host: State<'_, Host>,
    channel: Channel<HostEvent>,
) -> Result<(), String> {
    ensure_studio(webview.label())?;
    host.events.attach(Arc::new(ChannelSink(channel)));
    Ok(())
}

fn ensure_studio(label: &str) -> Result<(), String> {
    if label == STUDIO {
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
        assert!(ensure_studio(STUDIO).is_ok());
        let refused = ensure_studio("browser-ui").expect_err("refused");
        assert!(refused.contains("browser-ui"));
    }
}
