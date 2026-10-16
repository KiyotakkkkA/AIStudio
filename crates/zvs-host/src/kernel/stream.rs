use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use zvs_contract::{AppError, AppErrorCode, HostEvent, HostEventDraft, StreamId};

use crate::data::{DataError, Database, Repositories};
use crate::platform::events::StreamHandle;

pub type RunObserver = Arc<dyn Fn(&HostEvent) + Send + Sync>;

#[derive(Default)]
pub struct Observers {
    observers: Mutex<HashMap<String, Vec<(u64, RunObserver)>>>,
    next: AtomicU64,
}

impl Observers {
    pub fn add(&self, run_id: &str, observer: RunObserver) -> u64 {
        let token = self.next.fetch_add(1, Ordering::SeqCst);
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(run_id.to_owned())
            .or_default()
            .push((token, observer));
        token
    }

    pub fn remove(&self, run_id: &str, token: u64) {
        let mut observers = self
            .observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(list) = observers.get_mut(run_id) {
            list.retain(|(candidate, _)| *candidate != token);
            if list.is_empty() {
                observers.remove(run_id);
            }
        }
    }

    pub fn clear(&self) {
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub fn count(&self, run_id: &str) -> usize {
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .map_or(0, Vec::len)
    }

    fn notify(&self, run_id: &str, event: &HostEvent) {
        let ending = matches!(event.body, HostEventDraft::End { .. });
        let current: Vec<RunObserver> = {
            let mut observers = self
                .observers
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let current = observers
                .get(run_id)
                .map(|list| {
                    list.iter()
                        .map(|(_, observer)| Arc::clone(observer))
                        .collect()
                })
                .unwrap_or_default();
            if ending {
                observers.remove(run_id);
            }
            current
        };
        for observer in current {
            observer(event);
        }
    }
}

pub struct Subscription {
    observers: Arc<Observers>,
    run_id: String,
    token: u64,
}

impl Subscription {
    pub(crate) const fn new(observers: Arc<Observers>, run_id: String, token: u64) -> Self {
        Self {
            observers,
            run_id,
            token,
        }
    }

    pub fn unsubscribe(self) {
        self.observers.remove(&self.run_id, self.token);
    }
}

pub struct RunStream {
    run_id: String,
    handle: StreamHandle,
    database: Arc<Database>,
    observers: Arc<Observers>,
    gate: Mutex<()>,
}

impl RunStream {
    pub fn new(
        run_id: String,
        handle: StreamHandle,
        database: Arc<Database>,
        observers: Arc<Observers>,
    ) -> Self {
        Self {
            run_id,
            handle,
            database,
            observers,
            gate: Mutex::new(()),
        }
    }

    pub const fn stream_id(&self) -> StreamId {
        self.handle.id()
    }

    pub fn closed(&self) -> bool {
        self.handle.closed()
    }

    pub fn emit(&self, draft: HostEventDraft) -> Result<(), AppError> {
        self.commit(vec![draft], |_| Ok(()))
    }

    pub fn commit<T>(
        &self,
        drafts: Vec<HostEventDraft>,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        let _gate = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let mut value = None;
        let events = self.handle.publish_with(drafts, |events| {
            self.database.transaction(|transaction| {
                let repositories = Repositories::new(transaction);
                value = Some(work(&repositories)?);
                for event in events {
                    repositories.runs.append_event(&self.run_id, event)?;
                }
                Ok(())
            })
        })?;
        for event in &events {
            self.observers.notify(&self.run_id, event);
        }
        value.ok_or_else(|| AppError::new(AppErrorCode::Unknown, "The unit of work did not run"))
    }
}
