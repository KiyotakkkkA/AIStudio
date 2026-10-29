use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::broadcast;
use zvs_contract::CliNotice;

const CAPACITY: usize = 64;

type Focus = Arc<dyn Fn() + Send + Sync>;

struct Inner {
    sender: broadcast::Sender<CliNotice>,
    shown: Mutex<Vec<String>>,
    focus: Mutex<Option<Focus>>,
}

#[derive(Clone)]
pub struct CliNotices {
    inner: Arc<Inner>,
}

impl Default for CliNotices {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                sender: broadcast::channel(CAPACITY).0,
                shown: Mutex::new(Vec::new()),
                focus: Mutex::new(None),
            }),
        }
    }
}

impl CliNotices {
    pub fn verification(&self, origin: &str, shown: bool) {
        {
            let mut list = self
                .inner
                .shown
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            list.retain(|known| known != origin);
            if shown {
                list.push(origin.to_owned());
            }
        }
        let _ = self.inner.sender.send(CliNotice::AccountVerification {
            origin: origin.to_owned(),
            shown,
        });
    }

    pub fn current(&self) -> Vec<CliNotice> {
        self.inner
            .shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|origin| CliNotice::AccountVerification {
                origin: origin.clone(),
                shown: true,
            })
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CliNotice> {
        self.inner.sender.subscribe()
    }

    pub fn on_focus(&self, focus: impl Fn() + Send + Sync + 'static) {
        *self
            .inner
            .focus
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(focus));
    }

    pub fn focus(&self) {
        let focus = self
            .inner
            .focus
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(focus) = focus {
            focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const QWEN: &str = "https://chat.qwen.ai";

    #[tokio::test]
    async fn a_shown_check_is_broadcast_and_remembered_until_hidden() {
        let notices = CliNotices::default();
        let mut receiver = notices.subscribe();
        notices.verification(QWEN, true);
        assert_eq!(
            receiver.recv().await.expect("notice"),
            CliNotice::AccountVerification {
                origin: QWEN.to_owned(),
                shown: true
            }
        );
        assert_eq!(notices.current().len(), 1);
        notices.verification(QWEN, false);
        assert!(notices.current().is_empty());
    }

    #[test]
    fn focus_runs_the_registered_callback() {
        let notices = CliNotices::default();
        notices.focus();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        notices.on_focus(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        });
        notices.focus();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
