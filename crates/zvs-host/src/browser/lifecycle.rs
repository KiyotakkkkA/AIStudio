use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserLifecycleEvent {
    LinkFinished { url: String },
    LinkTabClosed { url: String },
    WorkspaceClosed,
    CookiesCleared { domain: String },
}

type Listener = Arc<dyn Fn(&BrowserLifecycleEvent) + Send + Sync>;

#[derive(Default)]
struct Listeners {
    next: AtomicU64,
    entries: Mutex<Vec<(u64, Listener)>>,
}

#[derive(Clone, Default)]
pub struct BrowserLifecycle {
    listeners: Arc<Listeners>,
}

#[must_use = "dropping the subscription unsubscribes the listener"]
pub struct LifecycleSubscription {
    id: u64,
    listeners: Weak<Listeners>,
}

impl Drop for LifecycleSubscription {
    fn drop(&mut self) {
        if let Some(listeners) = self.listeners.upgrade() {
            listeners
                .entries
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|(id, _)| *id != self.id);
        }
    }
}

impl BrowserLifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(
        &self,
        listener: impl Fn(&BrowserLifecycleEvent) + Send + Sync + 'static,
    ) -> LifecycleSubscription {
        let id = self.listeners.next.fetch_add(1, Ordering::Relaxed);
        self.listeners
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((id, Arc::new(listener)));
        LifecycleSubscription {
            id,
            listeners: Arc::downgrade(&self.listeners),
        }
    }

    pub fn emit(&self, event: &BrowserLifecycleEvent) {
        let listeners: Vec<Listener> = self
            .listeners
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        for listener in listeners {
            listener(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder(
        lifecycle: &BrowserLifecycle,
    ) -> (
        Arc<Mutex<Vec<BrowserLifecycleEvent>>>,
        LifecycleSubscription,
    ) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let subscription =
            lifecycle.subscribe(move |event| sink.lock().unwrap().push(event.clone()));
        (events, subscription)
    }

    #[test]
    fn every_listener_receives_each_event_in_order() {
        let lifecycle = BrowserLifecycle::new();
        let (first, _a) = recorder(&lifecycle);
        let (second, _b) = recorder(&lifecycle);
        lifecycle.emit(&BrowserLifecycleEvent::CookiesCleared {
            domain: ".example.com".into(),
        });
        lifecycle.emit(&BrowserLifecycleEvent::WorkspaceClosed);
        let expected = vec![
            BrowserLifecycleEvent::CookiesCleared {
                domain: ".example.com".into(),
            },
            BrowserLifecycleEvent::WorkspaceClosed,
        ];
        assert_eq!(*first.lock().unwrap(), expected);
        assert_eq!(*second.lock().unwrap(), expected);
    }

    #[test]
    fn dropping_the_subscription_unsubscribes() {
        let lifecycle = BrowserLifecycle::new();
        let (events, subscription) = recorder(&lifecycle);
        drop(subscription);
        lifecycle.emit(&BrowserLifecycleEvent::WorkspaceClosed);
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn a_listener_may_subscribe_while_an_event_is_delivered() {
        let lifecycle = BrowserLifecycle::new();
        let inner = lifecycle.clone();
        let nested = Arc::new(Mutex::new(Vec::new()));
        let keep = Arc::clone(&nested);
        let _outer = lifecycle.subscribe(move |_| {
            keep.lock().unwrap().push(inner.subscribe(|_| {}));
        });
        lifecycle.emit(&BrowserLifecycleEvent::LinkFinished {
            url: "https://chat.qwen.ai/".into(),
        });
        assert_eq!(nested.lock().unwrap().len(), 1);
    }
}
