use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;
use zvs_host::browser::automation::{
    AUTOMATION_FAILED, BLOCKED_ADDRESS, DOWNLOAD_FAILED, DOWNLOAD_NOT_STARTED, DownloadedFile,
    ELEMENT_DISABLED, ElementTarget, FOCUS_MOVED, NAVIGATION_FAILED, NO_FORM, NOT_A_LINK,
    NOT_EDITABLE, NOT_VISIBLE, OBSCURED, PageMatches, PageScreenshot, PageSnapshot, PageState,
    STALE_HANDLE, SUBMITS_FORM, UNKNOWN_HANDLE, conflict, failed, refused,
};
use zvs_host::browser::page::{self, AxNode, ElementRef, Handles};
use zvs_host::browser::{FieldFacts, is_web_url, navigation_url, secret_field, site_origin};
use zvs_host::kernel::types::cancelled;

use crate::browser::cdp::{CdpClient, CdpError, RemoteObject};

pub const WORLD: &str = "zvs-automation";
pub const OBJECT_GROUP: &str = "zvs-automation";
pub const DOMAINS: [&str; 4] = ["Page", "DOM", "Accessibility", "Overlay"];
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
pub const DOWNLOAD_START: Duration = Duration::from_secs(15);
pub const DOWNLOAD_FINISH: Duration = Duration::from_secs(600);
pub const SCREENSHOT_QUALITY: u8 = 70;

const FACTS: &str = r"function () {
  const submitter = (element) => {
    if (!element || !element.localName) return false;
    const type = String(element.type || '').toLowerCase();
    const control = (element.localName === 'button' && type === 'submit')
      || (element.localName === 'input' && (type === 'submit' || type === 'image'));
    return control && !!element.form;
  };
  const tag = this.localName || '';
  const type = String(this.type || '').toLowerCase();
  const text = ['', 'text', 'search', 'email', 'url', 'tel', 'number', 'password'];
  const attribute = (name) => (this.getAttribute && this.getAttribute(name)) || '';
  const labels = this.labels ? Array.from(this.labels).map((label) => label.textContent || '').join(' ') : '';
  const form = this.form || (this.closest ? this.closest('form') : null);
  const labelled = tag === 'label' ? this.control : null;
  return {
    tag,
    inputType: tag === 'input' ? type : '',
    name: attribute('name'),
    id: this.id || '',
    autocomplete: attribute('autocomplete'),
    placeholder: attribute('placeholder'),
    ariaLabel: attribute('aria-label'),
    label: labels,
    editable: !!this.isContentEditable || tag === 'textarea' || (tag === 'input' && text.includes(type)),
    disabled: !!this.disabled || attribute('aria-disabled') === 'true',
    readOnly: !!this.readOnly,
    inForm: !!form,
    submits: submitter(this) || submitter(labelled),
    href: (tag === 'a' || tag === 'area') ? String(this.href || '') : '',
    download: !!(this.hasAttribute && this.hasAttribute('download'))
  };
}";

const HIT: &str = r"function (x, y) {
  const submitter = (element) => {
    if (!element || !element.localName) return false;
    const type = String(element.type || '').toLowerCase();
    const control = (element.localName === 'button' && type === 'submit')
      || (element.localName === 'input' && (type === 'submit' || type === 'image'));
    return control && !!element.form;
  };
  const hit = document.elementFromPoint(x, y);
  if (!hit) return { inside: false, submits: false };
  const control = hit.closest ? hit.closest('button, input') : null;
  const label = hit.closest ? hit.closest('label') : null;
  return {
    inside: hit === this || (this.contains ? this.contains(hit) : false),
    submits: submitter(control) || submitter(label ? label.control : null)
  };
}";

const FOCUSED: &str = r"function (replace) {
  const active = document.activeElement;
  const focused = active === this || (this.contains ? this.contains(active) : false);
  if (focused && replace) {
    if (typeof this.select === 'function') {
      this.select();
    } else {
      const range = document.createRange();
      range.selectNodeContents(this);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
  }
  return focused;
}";

const SUBMIT: &str = r"function () {
  const form = this.localName === 'form' ? this : (this.form || (this.closest ? this.closest('form') : null));
  if (!form) return false;
  const type = String(this.type || '').toLowerCase();
  const own = this.form === form && ((this.localName === 'button' && type === 'submit')
    || (this.localName === 'input' && (type === 'submit' || type === 'image')));
  if (typeof form.requestSubmit === 'function') {
    form.requestSubmit(own ? this : null);
  } else {
    form.submit();
  }
  return true;
}";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn broken(error: &CdpError) -> AppError {
    failed(AUTOMATION_FAILED).with_detail("cause", error.to_string())
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, AppError> {
    serde_json::from_value(value)
        .map_err(|error| failed(AUTOMATION_FAILED).with_detail("cause", error.to_string()))
}

fn value_of<T: DeserializeOwned>(object: RemoteObject) -> Result<T, AppError> {
    decode(object.value.unwrap_or(Value::Null))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct History {
    current_index: usize,
    entries: Vec<HistoryEntry>,
}

#[derive(Debug, Deserialize)]
struct HistoryEntry {
    url: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
struct AxTree {
    nodes: Vec<AxNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Navigated {
    #[serde(default)]
    error_text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Captured {
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewport {
    client_width: f64,
    client_height: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutMetrics {
    css_visual_viewport: Viewport,
}

#[derive(Debug, Deserialize)]
struct Quads {
    quads: Vec<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct Hit {
    inside: bool,
    submits: bool,
}

struct Resolved {
    element: ElementRef,
    object_id: String,
}

fn pixels(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value.round() as u64
    } else {
        0
    }
}

fn center(quad: &[f64]) -> Option<(f64, f64)> {
    if quad.len() < 8 {
        return None;
    }
    let x = (quad[0] + quad[2] + quad[4] + quad[6]) / 4.0;
    let y = (quad[1] + quad[3] + quad[5] + quad[7]) / 4.0;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

pub struct Session {
    cdp: CdpClient,
    enabled: tokio::sync::Mutex<bool>,
    handles: Mutex<Handles>,
}

impl Session {
    pub fn new(cdp: CdpClient) -> Self {
        Self {
            cdp,
            enabled: tokio::sync::Mutex::new(false),
            handles: Mutex::default(),
        }
    }

    pub fn close(&self) {
        self.cdp.close();
    }

    pub fn target(&self, handle: &str) -> Option<ElementTarget> {
        lock(&self.handles)
            .get(handle)
            .map(|element| ElementTarget {
                origin: element.origin.clone(),
                role: element.role.clone(),
                name: element.name.clone(),
            })
    }

    async fn send(&self, method: &str, params: Value) -> Result<Value, AppError> {
        self.cdp
            .send(method, params)
            .await
            .map_err(|error| broken(&error))
    }

    pub async fn ensure_enabled(&self) -> Result<(), AppError> {
        let mut enabled = self.enabled.lock().await;
        if *enabled {
            return Ok(());
        }
        for domain in DOMAINS {
            self.send(&format!("{domain}.enable"), json!({})).await?;
        }
        *enabled = true;
        Ok(())
    }

    pub async fn end(&self) {
        let mut enabled = self.enabled.lock().await;
        if *enabled {
            for domain in DOMAINS.iter().rev() {
                let _ = self.cdp.send(&format!("{domain}.disable"), json!({})).await;
            }
            *enabled = false;
        }
        lock(&self.handles).clear();
    }

    pub async fn page(&self) -> Result<PageState, AppError> {
        let history: History = decode(self.send("Page.getNavigationHistory", json!({})).await?)?;
        Ok(history
            .entries
            .into_iter()
            .nth(history.current_index)
            .map(|entry| PageState {
                url: entry.url,
                title: entry.title,
            })
            .unwrap_or_default())
    }

    pub async fn navigate(&self, url: &str) -> Result<PageState, AppError> {
        let url = navigation_url(url).map_err(|_| refused(BLOCKED_ADDRESS))?;
        self.ensure_enabled().await?;
        let mut loaded = self
            .cdp
            .subscribe("Page.loadEventFired")
            .map_err(|error| broken(&error))?;
        let navigated: Navigated =
            decode(self.send("Page.navigate", json!({ "url": url })).await?)?;
        if let Some(error) = navigated.error_text.filter(|text| !text.is_empty()) {
            return Err(failed(NAVIGATION_FAILED).with_detail("cause", error));
        }
        let _ = tokio::time::timeout(LOAD_TIMEOUT, loaded.next()).await;
        self.page().await
    }

    async fn tree(&self) -> Result<(PageState, Vec<AxNode>), AppError> {
        self.ensure_enabled().await?;
        let page = self.page().await?;
        let tree: AxTree = decode(self.send("Accessibility.getFullAXTree", json!({})).await?)?;
        Ok((page, tree.nodes))
    }

    pub async fn read_page(&self) -> Result<PageSnapshot, AppError> {
        let (page, nodes) = self.tree().await?;
        let origin = site_origin(&page.url);
        let outline = match &origin {
            Some(origin) => page::outline(&nodes, Some((origin, &mut lock(&self.handles)))),
            None => page::outline(&nodes, None),
        };
        Ok(PageSnapshot {
            page,
            text: outline.text,
            elements: outline.elements,
            truncated: outline.truncated,
        })
    }

    pub async fn find(&self, query: &str, role: Option<&str>) -> Result<PageMatches, AppError> {
        let (page, nodes) = self.tree().await?;
        let matches = match site_origin(&page.url) {
            Some(origin) => page::find(&nodes, &origin, &mut lock(&self.handles), query, role),
            None => Vec::new(),
        };
        Ok(PageMatches { page, matches })
    }

    pub async fn screenshot(&self) -> Result<PageScreenshot, AppError> {
        self.ensure_enabled().await?;
        let page = self.page().await?;
        let metrics: LayoutMetrics = decode(self.send("Page.getLayoutMetrics", json!({})).await?)?;
        let captured: Captured = decode(
            self.send(
                "Page.captureScreenshot",
                json!({ "format": "jpeg", "quality": SCREENSHOT_QUALITY }),
            )
            .await?,
        )?;
        Ok(PageScreenshot {
            page,
            mime_type: "image/jpeg".to_owned(),
            width: pixels(metrics.css_visual_viewport.client_width),
            height: pixels(metrics.css_visual_viewport.client_height),
            data: captured.data,
        })
    }

    async fn resolve(&self, handle: &str) -> Result<Resolved, AppError> {
        self.ensure_enabled().await?;
        let element = lock(&self.handles)
            .get(handle)
            .cloned()
            .ok_or_else(|| conflict(UNKNOWN_HANDLE))?;
        let page = self.page().await?;
        if site_origin(&page.url).as_deref() != Some(element.origin.as_str()) {
            return Err(conflict(STALE_HANDLE));
        }
        let frame = self
            .cdp
            .main_frame()
            .await
            .map_err(|error| broken(&error))?;
        let world = self
            .cdp
            .create_isolated_world(&frame, WORLD)
            .await
            .map_err(|error| broken(&error))?;
        let object_id = self
            .cdp
            .resolve_node(element.backend_node_id, world, OBJECT_GROUP)
            .await
            .map_err(|error| match error {
                CdpError::Protocol { .. } => conflict(STALE_HANDLE),
                other => broken(&other),
            })?;
        Ok(Resolved { element, object_id })
    }

    async fn call<T: DeserializeOwned>(
        &self,
        resolved: &Resolved,
        declaration: &'static str,
        arguments: Vec<Value>,
    ) -> Result<T, AppError> {
        let object = self
            .cdp
            .call_on_object(&resolved.object_id, declaration, arguments)
            .await
            .map_err(|error| broken(&error))?;
        value_of(object)
    }

    async fn facts(&self, resolved: &Resolved) -> Result<FieldFacts, AppError> {
        let mut facts: FieldFacts = self.call(resolved, FACTS, Vec::new()).await?;
        if !resolved.element.name.is_empty() {
            if !facts.label.is_empty() {
                facts.label.push(' ');
            }
            facts.label.push_str(&resolved.element.name);
        }
        Ok(facts)
    }

    async fn press(&self, resolved: &Resolved) -> Result<(), AppError> {
        let backend = resolved.element.backend_node_id;
        let _ = self
            .send(
                "DOM.scrollIntoViewIfNeeded",
                json!({ "backendNodeId": backend }),
            )
            .await;
        let quads: Quads = decode(
            self.send("DOM.getContentQuads", json!({ "backendNodeId": backend }))
                .await?,
        )?;
        let (x, y) = quads
            .quads
            .iter()
            .find_map(|quad| center(quad))
            .ok_or_else(|| conflict(NOT_VISIBLE))?;
        let hit: Hit = self.call(resolved, HIT, vec![json!(x), json!(y)]).await?;
        if !hit.inside {
            return Err(refused(OBSCURED));
        }
        if hit.submits {
            return Err(refused(SUBMITS_FORM));
        }
        for (kind, buttons) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 0)] {
            self.send(
                "Input.dispatchMouseEvent",
                json!({
                    "type": kind,
                    "x": x,
                    "y": y,
                    "button": if kind == "mouseMoved" { "none" } else { "left" },
                    "buttons": buttons,
                    "clickCount": if kind == "mouseMoved" { 0 } else { 1 },
                }),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn highlight(&self, handle: Option<&str>) -> Result<(), AppError> {
        let Some(handle) = handle else {
            if *self.enabled.lock().await {
                self.send("Overlay.hideHighlight", json!({})).await?;
            }
            return Ok(());
        };
        self.ensure_enabled().await?;
        let element = lock(&self.handles)
            .get(handle)
            .cloned()
            .ok_or_else(|| conflict(UNKNOWN_HANDLE))?;
        let page = self.page().await?;
        if site_origin(&page.url).as_deref() != Some(element.origin.as_str()) {
            return Err(conflict(STALE_HANDLE));
        }
        self.send(
            "Overlay.highlightNode",
            json!({
                "backendNodeId": element.backend_node_id,
                "highlightConfig": {
                    "showInfo": false,
                    "contentColor": { "r": 133, "g": 170, "b": 214, "a": 0.12 },
                    "borderColor": { "r": 133, "g": 170, "b": 214, "a": 1.0 },
                },
            }),
        )
        .await
        .map(drop)
    }

    async fn release(&self) {
        self.cdp.release_group(OBJECT_GROUP).await;
    }

    pub async fn click(&self, handle: &str) -> Result<PageState, AppError> {
        let resolved = self.resolve(handle).await?;
        let outcome = async {
            let facts = self.facts(&resolved).await?;
            if facts.submits {
                return Err(refused(SUBMITS_FORM));
            }
            if facts.disabled {
                return Err(conflict(ELEMENT_DISABLED));
            }
            self.press(&resolved).await
        }
        .await;
        self.release().await;
        outcome?;
        self.page().await
    }

    pub async fn type_text(
        &self,
        handle: &str,
        text: &str,
        replace: bool,
    ) -> Result<PageState, AppError> {
        let resolved = self.resolve(handle).await?;
        let outcome = async {
            let facts = self.facts(&resolved).await?;
            if let Some(field) = secret_field(&facts) {
                return Err(refused(field.message()));
            }
            if !facts.editable {
                return Err(conflict(NOT_EDITABLE));
            }
            if facts.disabled || facts.read_only {
                return Err(conflict(ELEMENT_DISABLED));
            }
            self.send(
                "DOM.focus",
                json!({ "backendNodeId": resolved.element.backend_node_id }),
            )
            .await?;
            let focused: bool = self.call(&resolved, FOCUSED, vec![json!(replace)]).await?;
            if !focused {
                return Err(refused(FOCUS_MOVED));
            }
            let facts = self.facts(&resolved).await?;
            if let Some(field) = secret_field(&facts) {
                return Err(refused(field.message()));
            }
            if text.is_empty() {
                for kind in ["keyDown", "keyUp"] {
                    self.send(
                        "Input.dispatchKeyEvent",
                        json!({
                            "type": kind,
                            "key": "Backspace",
                            "code": "Backspace",
                            "windowsVirtualKeyCode": 8,
                        }),
                    )
                    .await?;
                }
            } else {
                self.send("Input.insertText", json!({ "text": text }))
                    .await?;
            }
            Ok(())
        }
        .await;
        self.release().await;
        outcome?;
        self.page().await
    }

    pub async fn submit(&self, handle: &str) -> Result<PageState, AppError> {
        let resolved = self.resolve(handle).await?;
        let outcome = self.call::<bool>(&resolved, SUBMIT, Vec::new()).await;
        self.release().await;
        if !outcome? {
            return Err(conflict(NO_FORM));
        }
        self.page().await
    }

    pub async fn download(
        &self,
        handle: &str,
        downloads: &Downloads,
        label: &str,
        cancel: &CancellationToken,
    ) -> Result<DownloadedFile, AppError> {
        let resolved = self.resolve(handle).await?;
        let pressed = async {
            let facts = self.facts(&resolved).await?;
            if !is_web_url(&facts.href) {
                return Err(conflict(NOT_A_LINK));
            }
            let armed = downloads.arm(label);
            self.press(&resolved).await?;
            Ok(armed)
        }
        .await;
        self.release().await;
        let (started, finished) = match pressed {
            Ok(armed) => armed,
            Err(error) => {
                downloads.disarm(label);
                return Err(error);
            }
        };
        let began = tokio::select! {
            started = tokio::time::timeout(DOWNLOAD_START, started) => started.ok().and_then(Result::ok),
            () = cancel.cancelled() => {
                downloads.disarm(label);
                return Err(cancelled());
            }
        };
        if began.is_none() {
            downloads.disarm(label);
            return Err(conflict(DOWNLOAD_NOT_STARTED));
        }
        let outcome = tokio::select! {
            outcome = tokio::time::timeout(DOWNLOAD_FINISH, finished) => outcome.ok().and_then(Result::ok),
            () = cancel.cancelled() => {
                downloads.disarm(label);
                return Err(cancelled());
            }
        };
        downloads.disarm(label);
        outcome.ok_or_else(|| failed(DOWNLOAD_FAILED))?
    }
}

type Finished = oneshot::Sender<Result<DownloadedFile, AppError>>;

struct Armed {
    started: Option<oneshot::Sender<()>>,
    finished: Option<Finished>,
    destination: Option<PathBuf>,
}

pub struct Downloads {
    folder: PathBuf,
    armed: Mutex<HashMap<String, Armed>>,
}

impl Downloads {
    pub fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            armed: Mutex::default(),
        }
    }

    pub fn arm(
        &self,
        label: &str,
    ) -> (
        oneshot::Receiver<()>,
        oneshot::Receiver<Result<DownloadedFile, AppError>>,
    ) {
        let (started, began) = oneshot::channel();
        let (finished, done) = oneshot::channel();
        lock(&self.armed).insert(
            label.to_owned(),
            Armed {
                started: Some(started),
                finished: Some(finished),
                destination: None,
            },
        );
        (began, done)
    }

    pub fn disarm(&self, label: &str) {
        lock(&self.armed).remove(label);
    }

    pub fn requested(&self, label: &str, url: &str, destination: &mut PathBuf) -> bool {
        let mut armed = lock(&self.armed);
        let Some(entry) = armed.get_mut(label) else {
            return false;
        };
        if entry.started.is_none()
            || !is_web_url(url)
            || std::fs::create_dir_all(&self.folder).is_err()
        {
            return false;
        }
        let Some(started) = entry.started.take() else {
            return false;
        };
        let name = file_name(destination, url);
        let target = unique_destination(&self.folder, &name);
        destination.clone_from(&target);
        entry.destination = Some(target);
        let _ = started.send(());
        true
    }

    pub fn finished(&self, label: &str, path: Option<&Path>, success: bool) {
        let mut armed = lock(&self.armed);
        let Some(entry) = armed.get_mut(label) else {
            return;
        };
        let Some(expected) = entry.destination.clone() else {
            return;
        };
        let Some(reply) = entry.finished.take() else {
            return;
        };
        let path = path.map_or(expected, Path::to_path_buf);
        let outcome = std::fs::metadata(&path)
            .ok()
            .filter(|metadata| success && metadata.is_file())
            .map(|metadata| DownloadedFile {
                file_name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: path.to_string_lossy().into_owned(),
                bytes: metadata.len(),
            })
            .ok_or_else(|| failed(DOWNLOAD_FAILED));
        let _ = reply.send(outcome);
    }
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
const FALLBACK_NAME: &str = "download";
const NAME_CHARS: usize = 120;

pub fn file_name(suggested: &Path, url: &str) -> String {
    let from_url = url::Url::parse(url).ok().and_then(|url| {
        url.path_segments()
            .and_then(|mut segments| segments.next_back().map(str::to_owned))
            .filter(|segment| !segment.is_empty())
    });
    let candidate = suggested
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .or(from_url)
        .unwrap_or_default();
    let cleaned: String = candidate
        .chars()
        .map(|character| {
            if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = cleaned
        .trim()
        .trim_end_matches(['.', ' '])
        .trim_start_matches('.');
    let short: String = trimmed.chars().take(NAME_CHARS).collect();
    let stem = short
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if short.is_empty() || RESERVED.contains(&stem.as_str()) {
        FALLBACK_NAME.to_owned()
    } else {
        short
    }
}

pub fn unique_destination(folder: &Path, name: &str) -> PathBuf {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| FALLBACK_NAME.to_owned());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let mut candidate = folder.join(name);
    let mut counter = 1;
    while candidate.exists() {
        candidate = folder.join(format!("{stem} ({counter}){extension}"));
        counter += 1;
    }
    candidate
}

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Arc<Session>>>);

impl Sessions {
    pub fn get(&self, label: &str) -> Option<Arc<Session>> {
        lock(&self.0).get(label).cloned()
    }

    pub fn open(&self, label: &str, connect: impl FnOnce() -> CdpClient) -> Arc<Session> {
        Arc::clone(
            lock(&self.0)
                .entry(label.to_owned())
                .or_insert_with(|| Arc::new(Session::new(connect()))),
        )
    }

    pub fn detach(&self, label: &str) {
        if let Some(session) = lock(&self.0).remove(label) {
            session.close();
        }
    }

    pub fn clear(&self) {
        for (_, session) in lock(&self.0).drain() {
            session.close();
        }
    }
}

#[cfg(test)]
mod tests;
