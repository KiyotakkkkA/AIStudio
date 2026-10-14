use std::sync::Arc;

use serde_json::{Map, Value, json};
use zvs_contract::HostEventDraft;

use super::events::EventBus;
use crate::drivers::ai::stream_to_events::ok_outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceRoute {
    Browser,
    Providers,
}

impl WorkspaceRoute {
    pub const fn path(self) -> &'static str {
        match self {
            Self::Browser => "/browser",
            Self::Providers => "/providers",
        }
    }
}

pub trait WorkspaceNavigator: Send + Sync {
    fn navigate(&self, route: WorkspaceRoute);
}

pub type SharedNavigator = Arc<dyn WorkspaceNavigator>;

pub fn navigation_step(route: WorkspaceRoute) -> Map<String, Value> {
    let mut step = Map::new();
    step.insert("domain".to_owned(), json!("navigation"));
    step.insert("path".to_owned(), json!(route.path()));
    step
}

impl WorkspaceNavigator for EventBus {
    fn navigate(&self, route: WorkspaceRoute) {
        let Ok(stream) = self.open_stream() else {
            return;
        };
        stream.emit(HostEventDraft::Step {
            step: navigation_step(route),
        });
        stream.end(ok_outcome());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::events::EventBusOptions;
    use crate::testing::MemorySink;

    #[test]
    fn navigating_emits_one_navigation_step_and_ends_the_stream() {
        let bus = EventBus::new(EventBusOptions::default());
        let sink = MemorySink::shared();
        bus.attach(sink.clone());
        bus.navigate(WorkspaceRoute::Providers);
        let events = sink.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].stream_id, events[1].stream_id);
        assert_eq!(
            events[0].body,
            HostEventDraft::Step {
                step: navigation_step(WorkspaceRoute::Providers)
            }
        );
        assert_eq!(
            serde_json::to_value(&events[0]).expect("serialises")["step"],
            json!({ "domain": "navigation", "path": "/providers" })
        );
        assert!(matches!(events[1].body, HostEventDraft::End { .. }));
        assert_eq!(WorkspaceRoute::Browser.path(), "/browser");
        bus.dispose();
        bus.navigate(WorkspaceRoute::Browser);
        assert_eq!(sink.events().len(), 2);
    }
}
