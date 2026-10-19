use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::Deserialize;
use tokio::sync::mpsc;

pub const MAX_PAYLOAD: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PageMessage {
    Head {
        status: u16,
        headers: Vec<(String, String)>,
        #[serde(default)]
        final_url: Option<String>,
        #[serde(default)]
        redirected: bool,
    },
    Chunk {
        data: String,
    },
    /// The page shows a verification dialog; the response stays pending until it is passed.
    Challenge,
    End,
    Error {
        aborted: bool,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
struct Envelope {
    id: String,
    #[serde(flatten)]
    message: PageMessage,
}

#[derive(Default)]
pub struct Routes {
    senders: Mutex<HashMap<String, mpsc::UnboundedSender<PageMessage>>>,
    failed: AtomicBool,
}

pub type SharedRoutes = Arc<Routes>;

impl Routes {
    pub fn shared() -> SharedRoutes {
        Arc::new(Self::default())
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, mpsc::UnboundedSender<PageMessage>>> {
        self.senders.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn register(&self, id: &str) -> Option<mpsc::UnboundedReceiver<PageMessage>> {
        if self.failed.load(Ordering::SeqCst) {
            return None;
        }
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut senders = self.lock();
        if senders.contains_key(id) {
            return None;
        }
        senders.insert(id.to_owned(), sender);
        Some(receiver)
    }

    pub fn unregister(&self, id: &str) {
        self.lock().remove(id);
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    pub fn deliver(&self, payload: &str) -> bool {
        if payload.len() > MAX_PAYLOAD {
            return false;
        }
        let Ok(envelope) = serde_json::from_str::<Envelope>(payload) else {
            return false;
        };
        let terminal = matches!(
            envelope.message,
            PageMessage::End | PageMessage::Error { .. }
        );
        let mut senders = self.lock();
        let Some(sender) = senders.get(&envelope.id) else {
            return false;
        };
        let delivered = sender.send(envelope.message).is_ok();
        if terminal || !delivered {
            senders.remove(&envelope.id);
        }
        delivered
    }

    pub fn fail_all(&self) {
        self.failed.store(true, Ordering::SeqCst);
        self.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn messages_reach_only_the_request_they_name() {
        let routes = Routes::shared();
        let mut first = routes.register("a").expect("registers");
        let mut second = routes.register("b").expect("registers");
        assert!(routes.register("a").is_none());
        assert!(
            routes.deliver(
                &json!({ "id": "a", "kind": "head", "status": 200, "headers": [["x", "1"]] })
                    .to_string()
            )
        );
        assert!(routes.deliver(&json!({ "id": "b", "kind": "chunk", "data": "AA==" }).to_string()));
        assert!(!routes.deliver(&json!({ "id": "zzz", "kind": "end" }).to_string()));
        assert_eq!(
            first.try_recv().expect("head"),
            PageMessage::Head {
                status: 200,
                headers: vec![("x".to_owned(), "1".to_owned())],
                final_url: None,
                redirected: false,
            }
        );
        assert!(first.try_recv().is_err());
        assert_eq!(
            second.try_recv().expect("chunk"),
            PageMessage::Chunk {
                data: "AA==".to_owned()
            }
        );
    }

    #[test]
    fn response_metadata_reaches_the_request() {
        let routes = Routes::shared();
        let mut receiver = routes.register("redirect").expect("registers");
        assert!(
            routes.deliver(
                &json!({
                    "id": "redirect", "kind": "head", "status": 200, "headers": [],
                    "final_url": "https://chat.qwen.ai/login", "redirected": true
                })
                .to_string()
            )
        );
        assert_eq!(
            receiver.try_recv().expect("head"),
            PageMessage::Head {
                status: 200,
                headers: Vec::new(),
                final_url: Some("https://chat.qwen.ai/login".to_owned()),
                redirected: true,
            }
        );
    }

    #[test]
    fn malformed_oversized_and_unknown_messages_are_dropped() {
        let routes = Routes::shared();
        let mut receiver = routes.register("a").expect("registers");
        for payload in [
            "not json".to_owned(),
            json!({ "id": "a", "kind": "teleport" }).to_string(),
            json!({ "id": "a", "kind": "head", "status": "200", "headers": [] }).to_string(),
            json!({ "id": 1, "kind": "end" }).to_string(),
            json!({ "id": "a", "kind": "chunk", "data": "A".repeat(MAX_PAYLOAD) }).to_string(),
        ] {
            assert!(!routes.deliver(&payload), "{payload:.60}");
        }
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_terminal_message_closes_the_route_and_failing_closes_them_all() {
        let routes = Routes::shared();
        let mut ended = routes.register("a").expect("registers");
        let mut open = routes.register("b").expect("registers");
        assert!(routes.deliver(&json!({ "id": "a", "kind": "end" }).to_string()));
        assert_eq!(ended.try_recv().expect("end"), PageMessage::End);
        assert!(!routes.deliver(&json!({ "id": "a", "kind": "end" }).to_string()));
        assert_eq!(routes.len(), 1);
        routes.fail_all();
        assert!(matches!(
            open.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ));
        assert!(routes.register("c").is_none());
    }
}
