use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Value, json};
use zvs_contract::{
    AppError, AppErrorCode, GraphBinding, GraphNode, RetryPolicy, RunGraph, ScenarioGraphDto,
    ScenarioIssueDto, ScenarioNodeDto, ScenarioOnError, ScenarioSlot,
};

use super::catalogue::{NodeSpec, ScenarioCatalogue, TOOL_SET, TOOL_SET_PARAM};
use super::service::SCENARIO_INVALID;
use super::validation::validate_graph;
use crate::kernel::project;

pub const NODE_NOT_IN_GRAPH: &str = "Шаг не найден в сценарии";
pub const NODE_NOT_RUNNABLE: &str = "Этот узел запускается только вместе с родительским шагом";
pub const AMBIGUOUS_RESULT: &str =
    "Ссылка на результат допустима только у шага с одним предыдущим шагом";
pub const PINNED_SOURCE: &str = "pinned";
pub const FROM_PREFIX: &str = "from:";
pub const EVENT_KEY: &str = "event";
pub const PARAMS_KEY: &str = "params";
pub const TOOLS_KEY: &str = "tools";

const RESULT_TEMPLATE: &str = "result";

fn invalid(issues: &[ScenarioIssueDto]) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, SCENARIO_INVALID)
        .with_detail("issues", json!(issues))
}

fn refused(message: &str, node_id: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message).with_detail("nodeId", node_id.to_owned())
}

pub fn from_key(node_id: &str) -> String {
    format!("{FROM_PREFIX}{node_id}")
}

pub fn slot_key(slot: ScenarioSlot) -> &'static str {
    match slot {
        ScenarioSlot::Model => "model",
        ScenarioSlot::Memory => "memory",
        ScenarioSlot::Tool => TOOLS_KEY,
        ScenarioSlot::Embedding => "embedding",
    }
}

pub fn result_reference(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    let inner = text.strip_prefix("{{")?.strip_suffix("}}")?.trim();
    if inner == RESULT_TEMPLATE {
        return Some(String::new());
    }
    inner
        .strip_prefix(RESULT_TEMPLATE)?
        .strip_prefix('.')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
}

struct Scope<'g> {
    graph: &'g ScenarioGraphDto,
    catalogue: &'g ScenarioCatalogue,
    nodes: HashMap<&'g str, &'g ScenarioNodeDto>,
}

impl<'g> Scope<'g> {
    fn new(
        graph: &'g ScenarioGraphDto,
        catalogue: &'g ScenarioCatalogue,
        focus: Option<&str>,
    ) -> Result<Self, AppError> {
        let relevant: Option<HashSet<&str>> = focus.map(|id| {
            graph
                .attachments
                .iter()
                .filter(|attachment| attachment.parent_node_id == id)
                .map(|attachment| attachment.child_node_id.as_str())
                .chain([id])
                .collect()
        });
        let issues: Vec<ScenarioIssueDto> = validate_graph(graph, catalogue)
            .into_iter()
            .filter(|issue| {
                relevant.as_ref().is_none_or(|relevant| {
                    issue
                        .node_id
                        .as_deref()
                        .is_some_and(|node| relevant.contains(node))
                })
            })
            .collect();
        if !issues.is_empty() {
            return Err(invalid(&issues));
        }
        Ok(Self {
            graph,
            catalogue,
            nodes: graph
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect(),
        })
    }

    fn node(&self, id: &str) -> Result<&'g ScenarioNodeDto, AppError> {
        self.nodes
            .get(id)
            .copied()
            .ok_or_else(|| refused(NODE_NOT_IN_GRAPH, id))
    }

    fn spec(&self, node: &ScenarioNodeDto) -> Result<&'g NodeSpec, AppError> {
        self.catalogue.spec(&node.node_type).ok_or_else(|| {
            invalid(&[ScenarioIssueDto::node(
                zvs_contract::ScenarioIssueRule::UnknownNodeType,
                &node.id,
            )])
        })
    }

    fn enabled_trunk(&self) -> Result<Vec<&'g ScenarioNodeDto>, AppError> {
        let attached: HashSet<&str> = self
            .graph
            .attachments
            .iter()
            .map(|attachment| attachment.child_node_id.as_str())
            .collect();
        let mut trunk = Vec::new();
        for node in &self.graph.nodes {
            if !node.is_disabled() && !attached.contains(node.id.as_str()) && self.spec(node)?.trunk
            {
                trunk.push(node);
            }
        }
        Ok(trunk)
    }

    fn predecessors(&self, id: &str) -> Vec<&'g str> {
        let mut found: Vec<&'g str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut pending: Vec<&'g str> = self.incoming(id);
        while let Some(source) = pending.pop() {
            if !seen.insert(source) {
                continue;
            }
            match self.nodes.get(source) {
                Some(node) if node.is_disabled() => pending.extend(self.incoming(source)),
                Some(_) => found.push(source),
                None => {}
            }
        }
        found.sort_unstable();
        found
    }

    fn incoming(&self, id: &str) -> Vec<&'g str> {
        self.graph
            .edges
            .iter()
            .filter(|edge| edge.to == id)
            .map(|edge| edge.from.as_str())
            .collect()
    }

    fn children(&self, id: &str) -> Vec<(ScenarioSlot, &'g ScenarioNodeDto)> {
        self.graph
            .attachments
            .iter()
            .filter(|attachment| attachment.parent_node_id == id)
            .filter_map(|attachment| {
                let child = self.nodes.get(attachment.child_node_id.as_str())?;
                (!child.is_disabled()).then_some((attachment.slot, *child))
            })
            .collect()
    }

    fn tool_keys(&self, child: &ScenarioNodeDto) -> Result<Vec<String>, AppError> {
        if child.node_type == TOOL_SET {
            return Ok(child
                .params
                .get(TOOL_SET_PARAM)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect());
        }
        Ok(self.spec(child)?.tool_key.clone().into_iter().collect())
    }

    fn attachments(&self, id: &str) -> Result<(Map<String, Value>, Vec<String>), AppError> {
        let mut config = Map::new();
        let mut keys: Vec<String> = Vec::new();
        for (slot, child) in self.children(id) {
            if slot == ScenarioSlot::Tool {
                for key in self.tool_keys(child)? {
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
                continue;
            }
            config.insert(
                slot_key(slot).to_owned(),
                json!({
                    "nodeId": child.id,
                    "type": child.node_type,
                    "params": child.params,
                }),
            );
        }
        if !keys.is_empty() {
            config.insert(TOOLS_KEY.to_owned(), json!(keys));
        }
        Ok((config, keys))
    }

    fn settings(&self, node: &ScenarioNodeDto, step: &mut GraphNode) {
        let settings = node.settings();
        if settings.max_attempts.is_some() || settings.backoff_ms.is_some() {
            step.retry = Some(RetryPolicy {
                max_attempts: settings.max_attempts.unwrap_or(1),
                backoff_ms: settings.backoff_ms.unwrap_or(0),
            });
        }
        step.timeout_ms = settings.timeout_ms;
        if settings.on_error() == ScenarioOnError::Continue {
            step.continue_on_error = Some(true);
        }
    }

    fn step(
        &self,
        node: &ScenarioNodeDto,
        upstream: Upstream<'_>,
    ) -> Result<(GraphNode, Vec<String>), AppError> {
        let spec = self.spec(node)?;
        if !spec.trunk {
            return Err(refused(NODE_NOT_RUNNABLE, &node.id));
        }
        let (config, keys) = self.attachments(&node.id)?;
        let mut bindings: BTreeMap<String, GraphBinding> = BTreeMap::new();
        let mut dependencies: Vec<String> = Vec::new();
        let input = if spec.builtin {
            let mut input = config;
            input.insert(PARAMS_KEY.to_owned(), Value::Object(node.params.clone()));
            bindings.insert(EVENT_KEY.to_owned(), GraphBinding::Run {});
            match upstream {
                Upstream::Steps(sources) => {
                    for source in sources {
                        bindings.insert(from_key(source), GraphBinding::node(*source));
                        dependencies.push((*source).to_owned());
                    }
                }
                Upstream::Pinned(value) => {
                    input.insert(from_key(PINNED_SOURCE), value.clone());
                }
            }
            input
        } else {
            let mut input = node.params.clone();
            for (key, value) in &node.params {
                let Some(path) = result_reference(value) else {
                    continue;
                };
                match upstream {
                    Upstream::Steps([source]) => {
                        bindings.insert(key.clone(), GraphBinding::node_path(*source, path));
                    }
                    Upstream::Steps(_) => return Err(refused(AMBIGUOUS_RESULT, &node.id)),
                    Upstream::Pinned(pinned) => {
                        input.insert(key.clone(), project(pinned, &path));
                    }
                }
            }
            if let Upstream::Steps(sources) = upstream {
                dependencies.extend(sources.iter().map(|source| (*source).to_owned()));
            }
            input
        };
        let mut step = GraphNode {
            dependencies: Some(dependencies),
            input: Some(Value::Object(input)),
            bindings: (!bindings.is_empty()).then_some(bindings),
            ..GraphNode::new(node.id.clone(), node.node_type.clone())
        };
        self.settings(node, &mut step);
        Ok((step, keys))
    }
}

#[derive(Clone, Copy)]
enum Upstream<'a> {
    Steps(&'a [&'a str]),
    Pinned(&'a Value),
}

fn run_graph(steps: Vec<GraphNode>, tools: Vec<String>) -> RunGraph {
    RunGraph {
        allowed_tools: Some(tools),
        ..RunGraph::new(steps)
    }
}

fn merge(tools: &mut Vec<String>, keys: Vec<String>) {
    for key in keys {
        if !tools.contains(&key) {
            tools.push(key);
        }
    }
}

pub fn compile(
    graph: &ScenarioGraphDto,
    catalogue: &ScenarioCatalogue,
) -> Result<RunGraph, AppError> {
    let scope = Scope::new(graph, catalogue, None)?;
    let mut steps = Vec::new();
    let mut tools = Vec::new();
    for node in scope.enabled_trunk()? {
        let sources = scope.predecessors(&node.id);
        let (step, keys) = scope.step(node, Upstream::Steps(&sources))?;
        merge(&mut tools, keys);
        steps.push(step);
    }
    Ok(run_graph(steps, tools))
}

pub fn compile_node(
    graph: &ScenarioGraphDto,
    catalogue: &ScenarioCatalogue,
    node_id: &str,
    input: &Value,
) -> Result<RunGraph, AppError> {
    let scope = Scope::new(graph, catalogue, Some(node_id))?;
    let node = scope.node(node_id)?;
    let (step, keys) = scope.step(node, Upstream::Pinned(input))?;
    Ok(run_graph(vec![step], keys))
}
