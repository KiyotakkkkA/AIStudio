use serde_json::Value;
use zvs_contract::{ScenarioSlot, ScenarioTriggerKind};

use crate::kernel::NodeRegistry;

pub const TRIGGER_CHAT: &str = "trigger.chat";
pub const TRIGGER_MAIL: &str = "trigger.mail";
pub const TRIGGER_SCHEDULE: &str = "trigger.schedule";
pub const TRIGGER_WEBHOOK: &str = "trigger.webhook";
pub const AGENT_TOOLS: &str = "agent.tools";
pub const MODEL_CHAT: &str = "model.chat";
pub const MEMORY_WINDOW: &str = "memory.window";
pub const EMBEDDING_MODEL: &str = "embedding.model";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotSpec {
    pub slot: ScenarioSlot,
    pub required: bool,
    pub multiple: bool,
}

impl SlotSpec {
    pub const fn required(slot: ScenarioSlot) -> Self {
        Self {
            slot,
            required: true,
            multiple: false,
        }
    }

    pub const fn optional(slot: ScenarioSlot) -> Self {
        Self {
            slot,
            required: false,
            multiple: false,
        }
    }

    pub const fn many(slot: ScenarioSlot) -> Self {
        Self {
            slot,
            required: false,
            multiple: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSpec {
    pub node_type: String,
    pub trunk: bool,
    pub trigger: Option<ScenarioTriggerKind>,
    pub fills: Vec<ScenarioSlot>,
    pub slots: Vec<SlotSpec>,
    pub required_params: Vec<String>,
    pub tool_key: Option<String>,
}

impl NodeSpec {
    pub fn action(node_type: &str) -> Self {
        Self {
            node_type: node_type.to_owned(),
            trunk: true,
            trigger: None,
            fills: Vec::new(),
            slots: Vec::new(),
            required_params: Vec::new(),
            tool_key: None,
        }
    }

    pub fn trigger(node_type: &str, kind: ScenarioTriggerKind) -> Self {
        Self {
            trigger: Some(kind),
            ..Self::action(node_type)
        }
    }

    pub fn sub_node(node_type: &str, slot: ScenarioSlot) -> Self {
        Self {
            trunk: false,
            fills: vec![slot],
            ..Self::action(node_type)
        }
    }

    #[must_use]
    pub fn with_slots(mut self, slots: &[SlotSpec]) -> Self {
        self.slots = slots.to_vec();
        self
    }

    #[must_use]
    pub fn with_required(mut self, params: &[&str]) -> Self {
        self.required_params = params.iter().map(|param| (*param).to_owned()).collect();
        self
    }

    pub fn slot(&self, slot: ScenarioSlot) -> Option<&SlotSpec> {
        self.slots.iter().find(|spec| spec.slot == slot)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioCatalogue {
    specs: Vec<NodeSpec>,
}

impl ScenarioCatalogue {
    pub fn builtin() -> Self {
        Self::default()
            .with(NodeSpec::trigger(TRIGGER_CHAT, ScenarioTriggerKind::Chat))
            .with(NodeSpec::trigger(TRIGGER_MAIL, ScenarioTriggerKind::Mail))
            .with(
                NodeSpec::trigger(TRIGGER_SCHEDULE, ScenarioTriggerKind::Schedule)
                    .with_required(&["cron"]),
            )
            .with(
                NodeSpec::trigger(TRIGGER_WEBHOOK, ScenarioTriggerKind::Webhook)
                    .with_required(&["path"]),
            )
            .with(NodeSpec::action(AGENT_TOOLS).with_slots(&[
                SlotSpec::required(ScenarioSlot::Model),
                SlotSpec::optional(ScenarioSlot::Memory),
                SlotSpec::many(ScenarioSlot::Tool),
            ]))
            .with(
                NodeSpec::sub_node(MODEL_CHAT, ScenarioSlot::Model)
                    .with_required(&["providerId", "modelId"]),
            )
            .with(NodeSpec::sub_node(MEMORY_WINDOW, ScenarioSlot::Memory))
            .with(
                NodeSpec::sub_node(EMBEDDING_MODEL, ScenarioSlot::Embedding)
                    .with_required(&["providerId", "modelId"]),
            )
    }

    pub fn from_registry(registry: &NodeRegistry) -> Self {
        registry
            .list()
            .iter()
            .fold(Self::builtin(), |catalogue, node| {
                let tool_key = node.permission().tool_key().map(str::to_owned);
                catalogue.with(NodeSpec {
                    fills: if tool_key.is_some() {
                        vec![ScenarioSlot::Tool]
                    } else {
                        Vec::new()
                    },
                    required_params: required_params(node.input_schema()),
                    tool_key,
                    ..NodeSpec::action(node.node_type())
                })
            })
    }

    #[must_use]
    pub fn with(mut self, spec: NodeSpec) -> Self {
        if self.spec(&spec.node_type).is_none() {
            self.specs.push(spec);
        }
        self
    }

    pub fn spec(&self, node_type: &str) -> Option<&NodeSpec> {
        self.specs.iter().find(|spec| spec.node_type == node_type)
    }
}

fn required_params(schema: &Value) -> Vec<String> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Vec::new();
    }
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
