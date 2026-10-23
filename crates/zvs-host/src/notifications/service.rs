use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value, json};
use zvs_contract::{
    AppError, AppErrorCode, ConversationId, HostEvent, HostEventDraft, NAVIGATION_STEP_DOMAIN,
    NotificationTargetDto, RunKind, RunOutcomeStatus, Timestamp,
};

use super::copy;
use crate::data::{Database, Repositories};
use crate::drivers::ai::stream_to_events::ok_outcome;
use crate::kernel::RunWatcher;
use crate::platform::clock::SharedClock;
use crate::platform::events::EventBus;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::platform::shell::{
    AUTOSTART_SETTING_KEY, NOTIFICATIONS_SETTING_KEY, NotificationDraft, NotificationKind,
    SharedShellPort, TRAY_SETTING_KEY, flag,
};
use crate::services::SettingService;

pub const COALESCE_WINDOW_MS: Timestamp = 5_000;
pub const ACTIVATION_REPEAT_MS: Timestamp = 2_000;

enum Delivery {
    Waiting(Option<NotificationTargetDto>),
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSubject {
    pub kind: RunKind,
    pub id: String,
    pub label: String,
}

pub struct NotificationServiceOptions {
    pub database: Arc<Database>,
    pub settings: SettingService,
    pub shell: SharedShellPort,
    pub events: EventBus,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl NotificationServiceOptions {
    pub fn new(
        database: Arc<Database>,
        settings: SettingService,
        shell: SharedShellPort,
        events: EventBus,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            settings,
            shell,
            events,
            clock,
            logger: silent_logger(),
        }
    }
}

struct Inner {
    database: Arc<Database>,
    settings: SettingService,
    shell: SharedShellPort,
    events: EventBus,
    clock: SharedClock,
    logger: SharedLogger,
    recent: Mutex<HashMap<(NotificationKind, String), Timestamp>>,
    activations: Mutex<HashMap<NotificationTargetDto, Timestamp>>,
    delivery: Mutex<Delivery>,
}

#[derive(Clone)]
pub struct NotificationService {
    inner: Arc<Inner>,
}

impl NotificationService {
    pub fn new(options: NotificationServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                database: options.database,
                settings: options.settings,
                shell: options.shell,
                events: options.events,
                clock: options.clock,
                logger: options.logger,
                recent: Mutex::new(HashMap::new()),
                activations: Mutex::new(HashMap::new()),
                delivery: Mutex::new(Delivery::Waiting(None)),
            }),
        }
    }

    pub fn watcher(&self) -> RunWatcher {
        let service = self.clone();
        Arc::new(move |run_id: &str, event: &HostEvent| service.observe(run_id, &event.body))
    }

    pub fn observe(&self, run_id: &str, event: &HostEventDraft) {
        if !matches!(
            event,
            HostEventDraft::Approval { .. } | HostEventDraft::End { .. }
        ) {
            return;
        }
        if let Some(subject) = self.subject(run_id) {
            self.handle(&subject, event);
        }
    }

    pub fn handle(&self, subject: &RunSubject, event: &HostEventDraft) {
        match (subject.kind, event) {
            (RunKind::Chat, HostEventDraft::Approval { request }) => {
                let Some(conversation_id) = ConversationId::parse(&subject.id) else {
                    return;
                };
                let tool = request
                    .get("subject")
                    .and_then(Value::as_str)
                    .map(|key| self.tool_name(key));
                self.raise(
                    NotificationKind::Approval,
                    &subject.id,
                    copy::APPROVAL_TITLE,
                    copy::approval_body(tool.as_deref(), &subject.label),
                    NotificationTargetDto::Conversation { conversation_id },
                );
            }
            (RunKind::Chat, HostEventDraft::End { outcome }) => {
                let Some(conversation_id) = ConversationId::parse(&subject.id) else {
                    return;
                };
                let title = match outcome.status {
                    RunOutcomeStatus::Ok => copy::TURN_DONE_TITLE,
                    RunOutcomeStatus::Failed => copy::TURN_FAILED_TITLE,
                    RunOutcomeStatus::Cancelled => return,
                };
                self.raise(
                    NotificationKind::Turn,
                    &subject.id,
                    title,
                    copy::clip(&subject.label),
                    NotificationTargetDto::Conversation { conversation_id },
                );
            }
            (RunKind::Download, HostEventDraft::End { outcome }) => {
                let title = match outcome.status {
                    RunOutcomeStatus::Ok => copy::DOWNLOAD_DONE_TITLE,
                    RunOutcomeStatus::Failed => copy::DOWNLOAD_FAILED_TITLE,
                    RunOutcomeStatus::Cancelled => return,
                };
                self.raise(
                    NotificationKind::Download,
                    &subject.id,
                    title,
                    copy::clip(&subject.label),
                    NotificationTargetDto::Downloads,
                );
            }
            _ => {}
        }
    }

    pub fn enabled(&self, kind: NotificationKind) -> bool {
        self.setting(NOTIFICATIONS_SETTING_KEY, true) && self.setting(kind.setting_key(), true)
    }

    fn raise(
        &self,
        kind: NotificationKind,
        tag: &str,
        title: &str,
        body: String,
        target: NotificationTargetDto,
    ) {
        if !self.enabled(kind) {
            return;
        }
        let now = self.inner.clock.now();
        let silent = {
            let mut recent = self
                .inner
                .recent
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            recent.retain(|_, at| now - *at < COALESCE_WINDOW_MS);
            recent.insert((kind, tag.to_owned()), now).is_some()
        };
        self.inner.shell.notify(NotificationDraft {
            kind,
            tag: tag.to_owned(),
            title: title.to_owned(),
            body,
            target,
            silent,
        });
    }

    pub fn activated(&self, target: NotificationTargetDto) {
        if self.repeated(target) {
            return;
        }
        {
            let mut delivery = self
                .inner
                .delivery
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Delivery::Waiting(pending) = &mut *delivery {
                *pending = Some(target);
                return;
            }
        }
        let Ok(stream) = self.inner.events.open_stream() else {
            return;
        };
        let mut step = Map::new();
        step.insert("domain".to_owned(), json!(NAVIGATION_STEP_DOMAIN));
        step.insert("path".to_owned(), json!(target.route()));
        if let NotificationTargetDto::Conversation { conversation_id } = target {
            step.insert("conversationId".to_owned(), json!(conversation_id));
        }
        stream.emit(HostEventDraft::Step { step });
        stream.end(ok_outcome());
    }

    pub fn take_pending(&self) -> Option<NotificationTargetDto> {
        let mut delivery = self
            .inner
            .delivery
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match std::mem::replace(&mut *delivery, Delivery::Live) {
            Delivery::Waiting(pending) => pending,
            Delivery::Live => None,
        }
    }

    fn repeated(&self, target: NotificationTargetDto) -> bool {
        let now = self.inner.clock.now();
        let mut activations = self
            .inner
            .activations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        activations.retain(|_, at| now - *at < ACTIVATION_REPEAT_MS);
        activations.insert(target, now).is_some()
    }

    pub fn clear(&self, target: NotificationTargetDto) {
        self.inner.shell.clear(target);
    }

    pub fn apply(&self, key: &str, value: &Value) -> Result<(), AppError> {
        match key {
            TRAY_SETTING_KEY => {
                self.inner.shell.set_tray(flag(Some(value), false));
                Ok(())
            }
            AUTOSTART_SETTING_KEY => self
                .inner
                .shell
                .set_autostart(flag(Some(value), false))
                .map_err(|error| {
                    self.inner.logger.log(
                        LogLevel::Warn,
                        "shell",
                        "Could not change the autostart registration",
                        &[("error", json!(error))],
                    );
                    AppError::new(AppErrorCode::NativeError, copy::AUTOSTART_FAILED)
                }),
            _ => Ok(()),
        }
    }

    pub fn start(&self) -> Result<(), AppError> {
        self.inner
            .shell
            .set_tray(self.setting(TRAY_SETTING_KEY, false));
        if let Some(registered) = self.inner.shell.autostart_enabled()
            && registered != self.setting(AUTOSTART_SETTING_KEY, false)
        {
            self.inner
                .settings
                .set(AUTOSTART_SETTING_KEY, &json!(registered))?;
        }
        Ok(())
    }

    fn setting(&self, key: &str, fallback: bool) -> bool {
        let stored = self.inner.settings.get(key).ok().flatten();
        flag(stored.as_ref().map(|setting| &setting.value), fallback)
    }

    fn subject(&self, run_id: &str) -> Option<RunSubject> {
        self.inner
            .database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let Some(run) = repositories.runs.get(run_id)? else {
                    return Ok(None);
                };
                let Some(id) = run.subject_id else {
                    return Ok(None);
                };
                let label = match run.kind {
                    RunKind::Chat => repositories.chat.get(&id)?.map(|row| row.title),
                    RunKind::Download => repositories
                        .downloads
                        .find_by_id(&id)?
                        .map(|row| row.display_name),
                    _ => None,
                };
                Ok(label.map(|label| RunSubject {
                    kind: run.kind,
                    id,
                    label,
                }))
            })
            .ok()
            .flatten()
    }

    fn tool_name(&self, key: &str) -> String {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).tools.find_by_key(key)?)
            })
            .ok()
            .flatten()
            .map_or_else(|| key.to_owned(), |row| row.display_name)
    }
}
