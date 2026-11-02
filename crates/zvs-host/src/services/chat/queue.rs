use std::collections::HashMap;
use std::sync::PoisonError;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CHAT_QUEUE_MAX, ChatCommand, ChatGoalDto, ChatQueueAddInput,
    ChatQueueDto, ChatQueueEditInput, ChatQueueInput, ChatQueueRemoveInput, ChatQueueStepDto,
    ChatQueuedMessageDto, ChatSendInput, ConversationId, HostEventDraft, RunHandleDto,
    RunOutcomeStatus, Validate,
};

use super::{ChatService, Inner};
use crate::platform::ids::new_id;
use crate::platform::logger::LogLevel;

pub const QUEUE_FULL: &str = "В очереди может быть не больше 10 сообщений";
pub const QUEUE_MESSAGE_NOT_FOUND: &str = "Сообщение в очереди не найдено";
pub const QUEUE_YIELDED: &str = "Turn ended at a safepoint to send queued messages";
const IDLE_POLL: Duration = Duration::from_millis(20);
const IDLE_ATTEMPTS: u32 = 250;

#[derive(Debug, Default)]
pub(crate) struct QueueState {
    messages: Vec<ChatQueuedMessageDto>,
    urgent: bool,
    yielded: bool,
}

pub(crate) type Queues = std::sync::Mutex<HashMap<String, QueueState>>;

fn not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, QUEUE_MESSAGE_NOT_FOUND)
}

impl Inner {
    fn with_queue<T>(&self, conversation: &str, work: impl FnOnce(&mut QueueState) -> T) -> T {
        let mut queues = self.queues.lock().unwrap_or_else(PoisonError::into_inner);
        let state = queues.entry(conversation.to_owned()).or_default();
        let result = work(state);
        if state.messages.is_empty() && !state.yielded {
            queues.remove(conversation);
        }
        result
    }

    pub(crate) fn queue_snapshot(&self, conversation: ConversationId) -> ChatQueueDto {
        let queues = self.queues.lock().unwrap_or_else(PoisonError::into_inner);
        queues.get(&conversation.to_string()).map_or_else(
            || ChatQueueDto::empty(conversation),
            |state| ChatQueueDto {
                conversation_id: conversation,
                messages: state.messages.clone(),
                urgent: state.urgent && !state.messages.is_empty(),
            },
        )
    }

    pub(crate) fn queue_yields(&self, conversation: &str) -> bool {
        let mut queues = self.queues.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(state) = queues.get_mut(conversation) else {
            return false;
        };
        if state.urgent && !state.messages.is_empty() {
            state.yielded = true;
            return true;
        }
        false
    }

    pub(crate) fn queue_yielded(&self, conversation: &str) -> bool {
        self.queues
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(conversation)
            .is_some_and(|state| state.yielded)
    }
}

impl ChatService {
    fn publish_queue(&self, queue: &ChatQueueDto) {
        let Ok(Value::Object(step)) = serde_json::to_value(ChatQueueStepDto::of(queue.clone()))
        else {
            return;
        };
        if let Ok(stream) = self.runs.events().open_stream() {
            stream.emit(HostEventDraft::Step { step });
        }
    }

    fn changed(&self, conversation: ConversationId) -> ChatQueueDto {
        let queue = self.inner.queue_snapshot(conversation);
        self.publish_queue(&queue);
        queue
    }

    fn idle(&self, conversation: &str) -> bool {
        self.inner.require_idle(conversation).is_ok()
    }

    pub fn queue(&self, input: &ChatQueueInput) -> Result<ChatQueueDto, AppError> {
        self.inner
            .conversation(&input.conversation_id.to_string())?;
        Ok(self.inner.queue_snapshot(input.conversation_id))
    }

    pub fn queue_add(&self, input: &ChatQueueAddInput) -> Result<ChatQueueDto, AppError> {
        input.validate()?;
        let key = input.conversation_id.to_string();
        self.inner.conversation(&key)?;
        let message = ChatQueuedMessageDto {
            id: new_id().hyphenated().to_string(),
            text: input.text().to_owned(),
            created_at: self.inner.now(),
        };
        self.inner.with_queue(&key, |state| {
            if state.messages.len() >= CHAT_QUEUE_MAX {
                return Err(AppError::new(AppErrorCode::Conflict, QUEUE_FULL)
                    .with_detail("limit", CHAT_QUEUE_MAX));
            }
            state.messages.push(message);
            Ok(())
        })?;
        let queue = self.changed(input.conversation_id);
        if self.idle(&key) && self.send_queued(input.conversation_id)?.is_some() {
            return Ok(self.inner.queue_snapshot(input.conversation_id));
        }
        Ok(queue)
    }

    pub fn queue_edit(&self, input: &ChatQueueEditInput) -> Result<ChatQueueDto, AppError> {
        input.validate()?;
        let key = input.conversation_id.to_string();
        self.inner.conversation(&key)?;
        self.inner.with_queue(&key, |state| {
            let message = state
                .messages
                .iter_mut()
                .find(|message| message.id == input.message_id)
                .ok_or_else(not_found)?;
            input.text().clone_into(&mut message.text);
            Ok::<(), AppError>(())
        })?;
        Ok(self.changed(input.conversation_id))
    }

    pub fn queue_remove(&self, input: &ChatQueueRemoveInput) -> Result<ChatQueueDto, AppError> {
        input.validate()?;
        let key = input.conversation_id.to_string();
        self.inner.conversation(&key)?;
        self.inner.with_queue(&key, |state| {
            let before = state.messages.len();
            state
                .messages
                .retain(|message| message.id != input.message_id);
            if state.messages.len() == before {
                return Err(not_found());
            }
            if state.messages.is_empty() {
                state.urgent = false;
            }
            Ok(())
        })?;
        Ok(self.changed(input.conversation_id))
    }

    pub fn queue_flush(&self, input: &ChatQueueInput) -> Result<ChatQueueDto, AppError> {
        let key = input.conversation_id.to_string();
        self.inner.conversation(&key)?;
        let pending = self.inner.with_queue(&key, |state| {
            if !state.messages.is_empty() {
                state.urgent = true;
            }
            !state.messages.is_empty()
        });
        if pending && self.idle(&key) {
            self.send_queued(input.conversation_id)?;
        }
        Ok(self.changed(input.conversation_id))
    }

    pub(crate) fn send_queued(
        &self,
        conversation: ConversationId,
    ) -> Result<Option<RunHandleDto>, AppError> {
        let key = conversation.to_string();
        let row = self.inner.conversation(&key)?;
        let continuing =
            self.inner.queue_yielded(&key) && row.goal.as_ref().is_some_and(ChatGoalDto::active);
        let (messages, yielded) = self.inner.with_queue(&key, |state| {
            state.urgent = false;
            (
                std::mem::take(&mut state.messages),
                std::mem::take(&mut state.yielded),
            )
        });
        let Some((last, preceding)) = messages.split_last() else {
            return Ok(None);
        };
        let input = ChatSendInput {
            commands: if continuing {
                vec![ChatCommand::Goal]
            } else {
                Vec::new()
            },
            ..ChatSendInput::new(conversation, last.text.clone())
        };
        let earlier: Vec<String> = preceding
            .iter()
            .map(|message| message.text.clone())
            .collect();
        match self.start_turn_queued(&input, &earlier) {
            Ok(handle) => {
                self.changed(conversation);
                Ok(Some(handle))
            }
            Err(error) => {
                self.inner.with_queue(&key, |state| {
                    let mut restored = messages.clone();
                    restored.append(&mut state.messages);
                    state.messages = restored;
                    state.yielded = yielded;
                });
                self.changed(conversation);
                Err(error)
            }
        }
    }

    pub(crate) fn queue_turn_ended(&self, conversation: &str, outcome: RunOutcomeStatus) {
        let Some(id) = ConversationId::parse(conversation) else {
            return;
        };
        let pending = self
            .inner
            .with_queue(conversation, |state| !state.messages.is_empty());
        if outcome != RunOutcomeStatus::Ok {
            self.inner.with_queue(conversation, |state| {
                state.urgent = false;
                state.yielded = false;
            });
            if pending {
                self.changed(id);
            }
            return;
        }
        if !pending {
            return;
        }
        let service = self.clone();
        let work = async move {
            for _ in 0..IDLE_ATTEMPTS {
                if service.idle(&id.to_string()) {
                    if let Err(error) = service.send_queued(id) {
                        service.inner.log(
                            LogLevel::Warn,
                            "Could not send the queued messages",
                            &[
                                ("conversationId", json!(id.to_string())),
                                ("error", json!(error.message)),
                            ],
                        );
                    }
                    return;
                }
                tokio::time::sleep(IDLE_POLL).await;
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn(work)),
            Err(_) => self.inner.log(
                LogLevel::Warn,
                "No runtime to send the queued messages",
                &[],
            ),
        }
    }
}
