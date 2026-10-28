use serde_json::{Map, Value, json};
use zvs_contract::{
    SCENARIO_FALSE_PORT, SCENARIO_MAIN_PORT, SCENARIO_TRUE_PORT, ScenarioNodeRole,
    ScenarioNodeTypeDto, ScenarioSlot, ScenarioSlotSpecDto, ScenarioTriggerKind,
};

use crate::kernel::NodeRegistry;

pub const TRIGGER_CHAT: &str = "trigger.chat";
pub const TRIGGER_MAIL: &str = "trigger.mail";
pub const TRIGGER_SCHEDULE: &str = "trigger.schedule";
pub const TRIGGER_WEBHOOK: &str = "trigger.webhook";
pub const TRIGGER_INTEGRATION: &str = "trigger.integration";
pub const AGENT_TOOLS: &str = "agent.tools";
pub const MODEL_CHAT: &str = "model.chat";
pub const MEMORY_WINDOW: &str = "memory.window";
pub const MEMORY_FACTS: &str = "memory.facts";
pub const KNOWLEDGE_STORES: &str = "knowledge.stores";
pub const STORES_PARAM: &str = "stores";
pub const POLICY_PARAM: &str = "policy";
pub const AREA_PARAM: &str = "area";
pub const POLICY_READ: &str = "read";
pub const POLICY_READ_WRITE: &str = "readWrite";
pub const AREA_GLOBAL: &str = "global";
pub const AREA_DIALOG: &str = "dialog";
pub const AREA_BOTH: &str = "both";
pub const FLOW_BRANCH: &str = "flow.branch";
pub const FLOW_MAP: &str = "flow.map";
pub const CONDITION_PARAM: &str = "condition";
pub const SCENARIO_ACTIONS: &[&str] = &[FLOW_BRANCH, FLOW_MAP];
pub const RESTRICTED_TOOL_PREFIXES: &[&str] = &["browser.", "vector.", "knowledge."];
pub const EMBEDDING_MODEL: &str = "embedding.model";
pub const TOOL_SET: &str = "tool.set";
pub const TOOL_SET_PARAM: &str = "tools";
pub const OUTPUT_CHAT_REPLY: &str = "output.chatReply";
pub const OUTPUT_CHAT_MESSAGE: &str = "output.chatMessage";
pub const OUTPUT_NEW_CHAT: &str = "output.newChat";
pub const OUTPUT_NOTIFY: &str = "output.notify";
pub const OUTPUT_FILE: &str = "output.file";
pub const TEMPLATE_PARAM: &str = "template";

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

#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub node_type: String,
    pub trunk: bool,
    pub trigger: Option<ScenarioTriggerKind>,
    pub fills: Vec<ScenarioSlot>,
    pub slots: Vec<SlotSpec>,
    pub ports: Vec<String>,
    pub required_params: Vec<String>,
    pub tool_key: Option<String>,
    pub params_schema: Value,
    pub idempotent: bool,
    pub builtin: bool,
}

impl NodeSpec {
    pub fn action(node_type: &str) -> Self {
        Self {
            node_type: node_type.to_owned(),
            trunk: true,
            trigger: None,
            fills: Vec::new(),
            slots: Vec::new(),
            ports: vec![SCENARIO_MAIN_PORT.to_owned()],
            required_params: Vec::new(),
            tool_key: None,
            params_schema: object_schema(&[], &[]),
            idempotent: false,
            builtin: false,
        }
    }

    pub fn trigger(node_type: &str, kind: ScenarioTriggerKind) -> Self {
        Self {
            trigger: Some(kind),
            idempotent: true,
            ..Self::action(node_type)
        }
    }

    pub fn sub_node(node_type: &str, slot: ScenarioSlot) -> Self {
        Self {
            trunk: false,
            fills: vec![slot],
            ports: Vec::new(),
            idempotent: true,
            ..Self::action(node_type)
        }
    }

    #[must_use]
    pub fn with_slots(mut self, slots: &[SlotSpec]) -> Self {
        self.slots = slots.to_vec();
        self
    }

    #[must_use]
    pub fn with_ports(mut self, ports: &[&str]) -> Self {
        self.ports = ports.iter().map(|port| (*port).to_owned()).collect();
        self
    }

    #[must_use]
    pub fn with_required(mut self, params: &[&str]) -> Self {
        self.required_params = params.iter().map(|param| (*param).to_owned()).collect();
        self
    }

    #[must_use]
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.required_params = required_params(&schema);
        self.params_schema = schema;
        self
    }

    #[must_use]
    pub const fn built_in(mut self) -> Self {
        self.builtin = true;
        self
    }

    pub fn slot(&self, slot: ScenarioSlot) -> Option<&SlotSpec> {
        self.slots.iter().find(|spec| spec.slot == slot)
    }

    pub const fn role(&self) -> ScenarioNodeRole {
        if self.trigger.is_some() {
            ScenarioNodeRole::Trigger
        } else if self.trunk {
            ScenarioNodeRole::Action
        } else {
            ScenarioNodeRole::SubNode
        }
    }

    pub fn dto(&self) -> ScenarioNodeTypeDto {
        ScenarioNodeTypeDto {
            node_type: self.node_type.clone(),
            role: self.role(),
            trigger: self.trigger,
            fills: self.fills.clone(),
            slots: self
                .slots
                .iter()
                .map(|spec| ScenarioSlotSpecDto {
                    slot: spec.slot,
                    required: spec.required,
                    multiple: spec.multiple,
                })
                .collect(),
            ports: self.ports.clone(),
            required_params: self.required_params.clone(),
            tool_key: self.tool_key.clone(),
            params_schema: self.params_schema.clone(),
            idempotent: self.idempotent,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioCatalogue {
    specs: Vec<NodeSpec>,
}

fn text() -> Value {
    json!({ "type": "string" })
}

fn bounded_integer(maximum: u64) -> Value {
    json!({ "type": "integer", "minimum": 1, "maximum": maximum })
}

fn model_schema() -> Value {
    object_schema(
        &[
            ("providerId", text()),
            ("modelId", text()),
            (
                "temperature",
                json!({ "type": "number", "minimum": 0, "maximum": 2 }),
            ),
        ],
        &["providerId", "modelId"],
    )
}

fn choice(options: &[&str]) -> Value {
    json!({ "type": "string", "enum": options })
}

fn output_schema(fields: &[(&str, Value)], required: &[&str]) -> Value {
    let mut all = fields.to_vec();
    all.push((TEMPLATE_PARAM, text()));
    object_schema(&all, required)
}

impl ScenarioCatalogue {
    pub fn builtin() -> Self {
        let specs = [
            NodeSpec::trigger(TRIGGER_CHAT, ScenarioTriggerKind::Chat)
                .with_schema(object_schema(&[("conversationId", text())], &[])),
            NodeSpec::trigger(TRIGGER_MAIL, ScenarioTriggerKind::Mail)
                .with_schema(object_schema(&[("from", text()), ("subject", text())], &[])),
            NodeSpec::trigger(TRIGGER_SCHEDULE, ScenarioTriggerKind::Schedule)
                .with_schema(object_schema(&[("cron", text())], &["cron"])),
            NodeSpec::trigger(TRIGGER_WEBHOOK, ScenarioTriggerKind::Webhook)
                .with_schema(object_schema(&[("path", text())], &["path"])),
            NodeSpec::trigger(TRIGGER_INTEGRATION, ScenarioTriggerKind::Integration).with_schema(
                object_schema(
                    &[("accountId", text()), ("trigger", text())],
                    &["accountId", "trigger"],
                ),
            ),
            NodeSpec::action(AGENT_TOOLS)
                .with_slots(&[
                    SlotSpec::required(ScenarioSlot::Model),
                    SlotSpec::many(ScenarioSlot::Memory),
                    SlotSpec::many(ScenarioSlot::Tool),
                ])
                .with_schema(object_schema(
                    &[
                        ("systemMessage", text()),
                        ("maxIterations", bounded_integer(20)),
                    ],
                    &[],
                )),
            NodeSpec::sub_node(MODEL_CHAT, ScenarioSlot::Model).with_schema(model_schema()),
            NodeSpec::sub_node(MEMORY_WINDOW, ScenarioSlot::Memory)
                .with_schema(object_schema(&[("size", bounded_integer(100))], &[])),
            NodeSpec::sub_node(MEMORY_FACTS, ScenarioSlot::Memory).with_schema(object_schema(
                &[
                    (POLICY_PARAM, choice(&[POLICY_READ, POLICY_READ_WRITE])),
                    (AREA_PARAM, choice(&[AREA_GLOBAL, AREA_DIALOG, AREA_BOTH])),
                ],
                &[],
            )),
            NodeSpec::sub_node(KNOWLEDGE_STORES, ScenarioSlot::Memory).with_schema(object_schema(
                &[(
                    STORES_PARAM,
                    json!({ "type": "array", "items": { "type": "string" } }),
                )],
                &[STORES_PARAM],
            )),
            NodeSpec::sub_node(TOOL_SET, ScenarioSlot::Tool).with_schema(object_schema(
                &[(
                    TOOL_SET_PARAM,
                    json!({ "type": "array", "items": { "type": "string" } }),
                )],
                &[TOOL_SET_PARAM],
            )),
            NodeSpec::sub_node(EMBEDDING_MODEL, ScenarioSlot::Embedding)
                .with_schema(model_schema()),
            NodeSpec::action(OUTPUT_CHAT_REPLY)
                .with_ports(&[])
                .with_schema(output_schema(&[], &[])),
            NodeSpec::action(OUTPUT_CHAT_MESSAGE)
                .with_ports(&[])
                .with_schema(output_schema(
                    &[("conversationId", text())],
                    &["conversationId"],
                )),
            NodeSpec::action(OUTPUT_NEW_CHAT)
                .with_ports(&[])
                .with_schema(output_schema(&[("title", text())], &[])),
            NodeSpec::action(OUTPUT_NOTIFY)
                .with_ports(&[])
                .with_schema(output_schema(&[("title", text())], &["title"])),
            NodeSpec::action(OUTPUT_FILE)
                .with_ports(&[])
                .with_schema(output_schema(&[("path", text())], &["path"])),
        ];
        specs
            .into_iter()
            .fold(Self::default(), |catalogue, spec| {
                catalogue.with(spec.built_in())
            })
            .with(NodeSpec {
                idempotent: true,
                ..NodeSpec::action(FLOW_BRANCH)
                    .with_ports(&[SCENARIO_TRUE_PORT, SCENARIO_FALSE_PORT])
                    .with_schema(object_schema(
                        &[(CONDITION_PARAM, json!({ "type": ["boolean", "string"] }))],
                        &[CONDITION_PARAM],
                    ))
            })
    }

    pub fn from_registry(registry: &NodeRegistry, actions: &[String]) -> Self {
        registry
            .list()
            .iter()
            .filter(|node| actions.iter().any(|action| action == node.node_type()))
            .fold(Self::builtin(), |catalogue, node| {
                let tool_key = node
                    .permission()
                    .tool_key()
                    .filter(|key| !crate::tools::NOT_TOOLS.contains(key))
                    .map(str::to_owned);
                catalogue.with(NodeSpec {
                    fills: if tool_key.is_some() {
                        vec![ScenarioSlot::Tool]
                    } else {
                        Vec::new()
                    },
                    tool_key,
                    idempotent: node.side_effect_free(),
                    ..NodeSpec::action(node.node_type()).with_schema(node.input_schema().clone())
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

    pub fn specs(&self) -> &[NodeSpec] {
        &self.specs
    }
}

pub fn object_schema(properties: &[(&str, Value)], required: &[&str]) -> Value {
    let properties: Map<String, Value> = properties
        .iter()
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect();
    json!({ "type": "object", "properties": properties, "required": required })
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
