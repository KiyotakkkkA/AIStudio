use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use zvs_contract::{
    BrowserAppearance, CliNotice, ComputerCommand, ComputerDisplay, ComputerPointer, ComputerState,
};
use zvs_host::cli::CliNotices;
use zvs_host::computer::{ComputerAgent, ComputerChange, ComputerChat};
use zvs_host::services::ChatService;

use crate::platform::overlay;
use crate::window::STUDIO;

pub const PANEL: &str = "computer-panel";
pub const GLOW: &str = "computer-glow";
const PAGE: &str = "computer-ui/index.html";
const PANEL_WIDTH: f64 = 460.0;
const PANEL_HEIGHT: f64 = 660.0;
const POINTER_TICK: Duration = Duration::from_millis(16);
const PANEL_COLLAPSED: f64 = 64.0;
const PANEL_VERIFYING: f64 = 116.0;
const PANEL_MARGIN: f64 = 24.0;

type Appearance = Arc<dyn Fn() -> BrowserAppearance + Send + Sync>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

enum Work {
    Change(ComputerChange),
    Verification,
    Engage(bool),
    Stop,
}

struct Inner {
    app: AppHandle,
    agent: ComputerAgent,
    appearance: Appearance,
    notices: CliNotices,
    work: Mutex<Sender<Work>>,
    subscribers: Mutex<Vec<Channel<ComputerState>>>,
    minimized: AtomicBool,
    placed: Mutex<Option<(ComputerDisplay, bool, bool)>>,
    pointers: Mutex<Vec<Channel<ComputerPointer>>>,
    pointing: AtomicU64,
}

#[derive(Clone)]
pub struct ComputerShell(Arc<Inner>);

pub struct ChatSender(pub ChatService);

impl ComputerChat for ChatSender {
    fn send(&self, conversation_id: &str, text: &str) -> Result<String, zvs_contract::AppError> {
        let id = zvs_contract::ConversationId::parse(conversation_id).ok_or_else(|| {
            zvs_contract::AppError::new(
                zvs_contract::AppErrorCode::NotFound,
                zvs_host::services::chat::CONVERSATION_NOT_FOUND,
            )
        })?;
        self.0
            .send(&zvs_contract::ChatSendInput::new(id, text))
            .map(|handle| handle.id.to_string())
    }
}

fn raw(window: &WebviewWindow) -> Option<isize> {
    window.hwnd().ok().map(|handle| handle.0 as isize)
}

impl ComputerShell {
    pub fn new(
        app: AppHandle,
        agent: ComputerAgent,
        appearance: Appearance,
        notices: CliNotices,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut verifications = notices.subscribe();
        let shell = Self(Arc::new(Inner {
            app,
            agent,
            appearance,
            notices,
            work: Mutex::new(sender),
            subscribers: Mutex::new(Vec::new()),
            minimized: AtomicBool::new(false),
            placed: Mutex::new(None),
            pointers: Mutex::new(Vec::new()),
            pointing: AtomicU64::new(0),
        }));
        let changes = lock(&shell.0.work).clone();
        shell.0.agent.subscribe(move |change| {
            let _ = changes.send(Work::Change(change));
        });
        let verified = lock(&shell.0.work).clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match verifications.recv().await {
                    Ok(CliNotice::AccountVerification { .. })
                    | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if verified.send(Work::Verification).is_err() {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        let worker = shell.clone();
        std::thread::Builder::new()
            .name("computer-overlay".to_owned())
            .spawn(move || worker.run(&receiver))
            .expect("the computer overlay thread starts");
        shell
    }

    fn post(&self, work: Work) {
        let _ = lock(&self.0.work).send(work);
    }

    fn run(&self, receiver: &Receiver<Work>) {
        while let Ok(work) = receiver.recv() {
            match work {
                Work::Change(ComputerChange::Started) => {
                    self.open();
                    self.publish();
                }
                Work::Change(ComputerChange::Updated) => {
                    self.place();
                    self.publish();
                }
                Work::Change(ComputerChange::Ended) => {
                    self.close();
                    self.publish();
                }
                Work::Verification => {
                    self.place();
                    self.publish();
                }
                Work::Engage(engaged) => self.engage(engaged),
                Work::Stop => {
                    let agent = self.0.agent.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = agent.stop().await;
                    });
                }
            }
        }
    }

    pub fn state(&self) -> ComputerState {
        let mut state = self.0.agent.state();
        state.appearance = (self.0.appearance)();
        state.verifications = self
            .0
            .notices
            .current()
            .into_iter()
            .filter_map(|notice| match notice {
                CliNotice::AccountVerification {
                    origin,
                    shown: true,
                } => Some(origin),
                _ => None,
            })
            .collect();
        state
    }

    pub fn publish(&self) {
        let state = self.state();
        lock(&self.0.subscribers).retain(|channel| channel.send(state.clone()).is_ok());
    }

    pub fn subscribe(&self, channel: Channel<ComputerState>) {
        if channel.send(self.state()).is_ok() {
            lock(&self.0.subscribers).push(channel);
        }
    }

    pub fn watch_pointer(&self, channel: Channel<ComputerPointer>) {
        lock(&self.0.pointers).push(channel);
    }

    fn origin(&self) -> (i64, i64) {
        lock(&self.0.placed)
            .as_ref()
            .map_or((0, 0), |(display, _, _)| (display.x, display.y))
    }

    fn track_pointer(&self) {
        let generation = self.0.pointing.fetch_add(1, Ordering::SeqCst) + 1;
        let shell = self.clone();
        let _ = std::thread::Builder::new()
            .name("computer-pointer".to_owned())
            .spawn(move || {
                let mut last = None;
                while shell.0.pointing.load(Ordering::SeqCst) == generation {
                    if let Some((x, y, down)) = overlay::pointer() {
                        let (left, top) = shell.origin();
                        let pointer = ComputerPointer {
                            x: i64::from(x) - left,
                            y: i64::from(y) - top,
                            down,
                        };
                        if last != Some(pointer) {
                            last = Some(pointer);
                            lock(&shell.0.pointers).retain(|channel| channel.send(pointer).is_ok());
                        }
                    }
                    std::thread::sleep(POINTER_TICK);
                }
            });
    }

    fn build(&self, label: &str) -> Option<WebviewWindow> {
        if let Some(existing) = self.0.app.get_webview_window(label) {
            return Some(existing);
        }
        let window = WebviewWindowBuilder::new(&self.0.app, label, WebviewUrl::App(PAGE.into()))
            .title("ZVS AI Studio")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .focused(false)
            .visible(false)
            .build()
            .ok()?;
        if let Some(handle) = raw(&window) {
            overlay::hide_from_capture(handle);
            overlay::set_no_activate(handle, true);
        }
        Some(window)
    }

    fn open(&self) {
        if let Some(studio) = self.0.app.get_webview_window(STUDIO)
            && !studio.is_minimized().unwrap_or(true)
            && studio.minimize().is_ok()
        {
            self.0.minimized.store(true, Ordering::SeqCst);
        }
        let glow = self.build(GLOW);
        if let Some(glow) = &glow {
            let _ = glow.set_ignore_cursor_events(true);
        }
        self.build(PANEL);
        *lock(&self.0.placed) = None;
        self.place();
        for window in [GLOW, PANEL]
            .iter()
            .filter_map(|label| self.0.app.get_webview_window(label))
        {
            if let Some(handle) = raw(&window) {
                overlay::show_inactive(handle);
            }
        }
        let shell = self.clone();
        overlay::watch_stop_key(move || shell.post(Work::Stop));
        self.track_pointer();
    }

    fn place(&self) {
        let state = self.0.agent.state();
        let Some(display) = state.display.or_else(|| self.primary()) else {
            return;
        };
        let verifying = !self.0.notices.current().is_empty();
        let placed = Some((display, state.collapsed, verifying));
        if *lock(&self.0.placed) == placed {
            return;
        }
        *lock(&self.0.placed) = placed;
        if let Some(glow) = self.0.app.get_webview_window(GLOW) {
            let _ = glow.set_position(PhysicalPosition::new(display.x as f64, display.y as f64));
            let _ = glow.set_size(PhysicalSize::new(
                display.width as f64,
                display.height as f64,
            ));
        }
        if let Some(panel) = self.0.app.get_webview_window(PANEL) {
            let scale = panel.scale_factor().unwrap_or(1.0);
            let height = match (state.collapsed, verifying) {
                (false, _) => PANEL_HEIGHT,
                (true, false) => PANEL_COLLAPSED,
                (true, true) => PANEL_VERIFYING,
            };
            let width = (PANEL_WIDTH * scale).round();
            let tall = (height * scale).round();
            let margin = (PANEL_MARGIN * scale).round();
            let right = display.x as f64 + display.width as f64;
            let bottom = display.y as f64 + display.height as f64;
            let _ = panel.set_size(PhysicalSize::new(width as u32, tall as u32));
            let _ = panel.set_position(PhysicalPosition::new(
                (right - width - margin) as i32,
                (bottom - tall - margin * 3.0) as i32,
            ));
        }
    }

    fn primary(&self) -> Option<ComputerDisplay> {
        let monitor = self.0.app.primary_monitor().ok().flatten()?;
        Some(ComputerDisplay {
            x: i64::from(monitor.position().x),
            y: i64::from(monitor.position().y),
            width: u64::from(monitor.size().width),
            height: u64::from(monitor.size().height),
        })
    }

    fn close(&self) {
        self.0.pointing.fetch_add(1, Ordering::SeqCst);
        lock(&self.0.pointers).clear();
        overlay::release_stop_key();
        for label in [GLOW, PANEL] {
            if let Some(window) = self.0.app.get_webview_window(label) {
                let _ = window.destroy();
            }
        }
        *lock(&self.0.placed) = None;
        if self.0.minimized.swap(false, Ordering::SeqCst)
            && let Some(studio) = self.0.app.get_webview_window(STUDIO)
        {
            let _ = studio.unminimize();
        }
    }

    fn engage(&self, engaged: bool) {
        let Some(panel) = self.0.app.get_webview_window(PANEL) else {
            return;
        };
        if let Some(handle) = raw(&panel) {
            overlay::set_no_activate(handle, !engaged);
        }
        if engaged {
            let _ = panel.set_focus();
        }
    }

    pub async fn command(&self, command: ComputerCommand) -> Result<(), String> {
        let agent = &self.0.agent;
        let result = match command {
            ComputerCommand::State => {
                self.publish();
                Ok(())
            }
            ComputerCommand::Stop => agent.stop().await,
            ComputerCommand::Approve { id } => agent.approve(&id),
            ComputerCommand::Deny { id } => agent.deny(&id),
            ComputerCommand::Answer { id, answer } => agent.answer_current(&id, answer),
            ComputerCommand::Say { text } => {
                self.post(Work::Engage(false));
                agent.say(&text).await
            }
            ComputerCommand::Engage => {
                self.post(Work::Engage(true));
                Ok(())
            }
            ComputerCommand::Release => {
                self.post(Work::Engage(false));
                Ok(())
            }
            ComputerCommand::Collapse { collapsed } => {
                agent.collapse(collapsed);
                Ok(())
            }
            ComputerCommand::Verify => {
                self.0.notices.focus();
                Ok(())
            }
        };
        result.map_err(|error| error.message)
    }
}
