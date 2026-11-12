use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::{Map, json};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, CLI_STEP_DOMAIN, CliChangeDto, CliConnectionDto, CliConsentDto,
    CliNotice, HostEventDraft, RunOutcomeDto, RunOutcomeStatus, StreamId,
};

use crate::platform::clock::SharedClock;
use crate::platform::events::EventBus;
use crate::platform::ids::new_id;
use crate::platform::shell::SharedShellPort;

pub const CLI_CONSENT_TIMEOUT: Duration = Duration::from_secs(60);
pub const CLI_STUDIO_REQUIRED: &str = "Изменение требует подтверждения в окне приложения.";
pub const CLI_CONSENT_DECLINED: &str = "Изменение отклонено в приложении";
pub const CLI_CONSENT_EXPIRED: &str = "Время ожидания подтверждения истекло";
pub const CLI_CONSENT_MISSING: &str = "Запрос на подтверждение не найден";
pub const CLI_CONNECTION_CLOSED: &str = "Подключение консоли закрыто";
pub const CLI_CONNECTION_MISSING: &str = "Подключение консоли не найдено";
const STREAM_CACHE_MAX: usize = 4096;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliClient {
    pub pid: Option<u32>,
    pub image: Option<String>,
    pub recognised: bool,
}

pub type NoticeSender = Arc<dyn Fn(CliNotice) + Send + Sync>;

struct Entry {
    client: CliClient,
    connected_at: i64,
    owned: HashSet<String>,
    streams: HashMap<StreamId, Option<String>>,
    closed: CancellationToken,
    notify: NoticeSender,
}

struct Held {
    dto: CliConsentDto,
    answer: oneshot::Sender<bool>,
}

#[derive(Default)]
struct State {
    next: u64,
    connections: BTreeMap<u64, Entry>,
    released: HashSet<String>,
    consents: BTreeMap<String, Held>,
}

struct Inner {
    clock: SharedClock,
    events: EventBus,
    shell: SharedShellPort,
    timeout: Duration,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct CliConnections {
    inner: Arc<Inner>,
}

fn refused(message: &str) -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, message)
}

impl CliConnections {
    pub fn new(clock: SharedClock, events: EventBus, shell: SharedShellPort) -> Self {
        Self::with_timeout(clock, events, shell, CLI_CONSENT_TIMEOUT)
    }

    pub fn with_timeout(
        clock: SharedClock,
        events: EventBus,
        shell: SharedShellPort,
        timeout: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                clock,
                events,
                shell,
                timeout,
                state: Mutex::new(State::default()),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn changed(&self) {
        let Ok(stream) = self.inner.events.open_stream() else {
            return;
        };
        let mut step = Map::new();
        step.insert("domain".to_owned(), json!(CLI_STEP_DOMAIN));
        stream.emit(HostEventDraft::Step { step });
        stream.end(RunOutcomeDto {
            status: RunOutcomeStatus::Ok,
            code: None,
            message: None,
        });
    }

    pub fn open(&self, client: CliClient, notify: NoticeSender) -> (u64, CancellationToken) {
        let closed = CancellationToken::new();
        let id = {
            let mut state = self.state();
            state.next += 1;
            let id = state.next;
            state.connections.insert(
                id,
                Entry {
                    client,
                    connected_at: self.inner.clock.now(),
                    owned: HashSet::new(),
                    streams: HashMap::new(),
                    closed: closed.clone(),
                    notify,
                },
            );
            id
        };
        self.changed();
        (id, closed)
    }

    pub fn close(&self, id: u64) {
        let removed = {
            let mut state = self.state();
            let entry = state.connections.remove(&id);
            state
                .consents
                .retain(|_, held| held.dto.connection_id != id);
            entry
        };
        if let Some(entry) = removed {
            entry.closed.cancel();
            self.changed();
        }
    }

    pub fn disconnect(&self, id: u64) -> Result<(), AppError> {
        let state = self.state();
        let entry = state
            .connections
            .get(&id)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CLI_CONNECTION_MISSING))?;
        entry.closed.cancel();
        Ok(())
    }

    pub fn list(&self) -> Vec<CliConnectionDto> {
        self.state()
            .connections
            .iter()
            .map(|(id, entry)| CliConnectionDto {
                id: *id,
                pid: entry.client.pid,
                image: entry.client.image.clone(),
                recognised: entry.client.recognised,
                connected_at: entry.connected_at,
                dialogs: u32::try_from(entry.owned.len()).unwrap_or(u32::MAX),
            })
            .collect()
    }

    pub fn ensure_open(&self, id: u64) -> Result<(), AppError> {
        if self.state().connections.contains_key(&id) {
            Ok(())
        } else {
            Err(refused(CLI_CONNECTION_CLOSED))
        }
    }

    pub fn owns(&self, id: u64, dialog: &str) -> bool {
        self.state()
            .connections
            .get(&id)
            .is_some_and(|entry| entry.owned.contains(dialog))
    }

    pub fn adopt(&self, id: u64, dialog: &str) {
        let adopted = {
            let mut state = self.state();
            state.connections.get_mut(&id).is_some_and(|entry| {
                entry.streams.clear();
                entry.owned.insert(dialog.to_owned())
            })
        };
        if adopted {
            self.changed();
        }
    }

    pub fn released(&self, dialog: &str) -> bool {
        self.state().released.contains(dialog)
    }

    pub fn release(&self, dialog: &str) {
        self.state().released.insert(dialog.to_owned());
    }

    pub fn sees(
        &self,
        id: u64,
        stream: StreamId,
        resolve: impl FnOnce() -> Option<String>,
    ) -> bool {
        let cached = {
            let state = self.state();
            let Some(entry) = state.connections.get(&id) else {
                return false;
            };
            entry.streams.get(&stream).cloned()
        };
        let dialog = match cached {
            Some(dialog) => dialog,
            None => {
                let dialog = resolve();
                let mut state = self.state();
                if let Some(entry) = state.connections.get_mut(&id) {
                    if entry.streams.len() >= STREAM_CACHE_MAX {
                        entry.streams.clear();
                    }
                    entry.streams.insert(stream, dialog.clone());
                }
                dialog
            }
        };
        dialog.is_some_and(|dialog| self.owns(id, &dialog))
    }

    pub fn client(&self, id: u64) -> Option<CliClient> {
        self.state()
            .connections
            .get(&id)
            .map(|entry| entry.client.clone())
    }

    fn notify(&self, id: u64) {
        let (notify, pending) = {
            let state = self.state();
            let Some(entry) = state.connections.get(&id) else {
                return;
            };
            let pending = state
                .consents
                .values()
                .any(|held| held.dto.connection_id == id);
            (Arc::clone(&entry.notify), pending)
        };
        notify(CliNotice::Consent { pending });
    }

    pub async fn confirm(&self, id: u64, change: CliChangeDto) -> Result<(), AppError> {
        if !self.inner.events.attached() {
            return Err(refused(CLI_STUDIO_REQUIRED));
        }
        let client = self
            .client(id)
            .ok_or_else(|| refused(CLI_CONNECTION_CLOSED))?;
        let created_at = self.inner.clock.now();
        let timeout = i64::try_from(self.inner.timeout.as_millis()).unwrap_or(i64::MAX);
        let consent = new_id().to_string();
        let (sender, receiver) = oneshot::channel();
        self.state().consents.insert(
            consent.clone(),
            Held {
                dto: CliConsentDto {
                    id: consent.clone(),
                    connection_id: id,
                    change,
                    recognised: client.recognised,
                    image: client.image,
                    created_at,
                    expires_at: created_at.saturating_add(timeout),
                },
                answer: sender,
            },
        );
        self.inner.shell.show_studio();
        self.notify(id);
        self.changed();
        let answer = tokio::time::timeout(self.inner.timeout, receiver).await;
        self.state().consents.remove(&consent);
        self.notify(id);
        self.changed();
        match answer {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(refused(CLI_CONSENT_DECLINED)),
            Ok(Err(_)) => Err(refused(CLI_CONNECTION_CLOSED)),
            Err(_) => Err(refused(CLI_CONSENT_EXPIRED)),
        }
    }

    pub fn pending(&self) -> Vec<CliConsentDto> {
        let mut pending: Vec<CliConsentDto> = self
            .state()
            .consents
            .values()
            .map(|held| held.dto.clone())
            .collect();
        pending.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        pending
    }

    pub fn answer(&self, consent: &str, confirm: bool) -> Result<(), AppError> {
        let held = self
            .state()
            .consents
            .remove(consent)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CLI_CONSENT_MISSING))?;
        held.answer
            .send(confirm)
            .map_err(|_| AppError::new(AppErrorCode::NotFound, CLI_CONSENT_MISSING))
    }
}

#[cfg(test)]
mod tests;
