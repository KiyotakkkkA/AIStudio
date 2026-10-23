use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;
use zvs_contract::{
    ScenarioAttachmentDto, ScenarioGraphDto, ScenarioIssueDto, ScenarioIssueRule, ScenarioNodeDto,
    ScenarioSlot, ScenarioTriggerDto, validate::js_trim,
};

use super::catalogue::{NodeSpec, ScenarioCatalogue};

#[derive(Default)]
struct Issues(Vec<ScenarioIssueDto>);

impl Issues {
    fn push(&mut self, issue: ScenarioIssueDto) {
        if !self.0.contains(&issue) {
            self.0.push(issue);
        }
    }

    fn node(&mut self, rule: ScenarioIssueRule, node_id: &str) {
        self.push(ScenarioIssueDto::node(rule, node_id));
    }
}

struct Graph<'g> {
    order: Vec<&'g ScenarioNodeDto>,
    nodes: HashMap<&'g str, &'g ScenarioNodeDto>,
    specs: HashMap<&'g str, &'g NodeSpec>,
}

impl<'g> Graph<'g> {
    fn new(
        graph: &'g ScenarioGraphDto,
        catalogue: &'g ScenarioCatalogue,
        issues: &mut Issues,
    ) -> Self {
        let mut order = Vec::new();
        let mut nodes = HashMap::new();
        let mut specs = HashMap::new();
        for node in &graph.nodes {
            if nodes.contains_key(node.id.as_str()) {
                issues.node(ScenarioIssueRule::DuplicateNode, &node.id);
                continue;
            }
            nodes.insert(node.id.as_str(), node);
            order.push(node);
            match catalogue.spec(&node.node_type) {
                Some(spec) => {
                    specs.insert(node.id.as_str(), spec);
                }
                None => issues.node(ScenarioIssueRule::UnknownNodeType, &node.id),
            }
        }
        Self {
            order,
            nodes,
            specs,
        }
    }

    fn has(&self, id: &str) -> bool {
        self.nodes.contains_key(id)
    }

    fn spec(&self, id: &str) -> Option<&'g NodeSpec> {
        self.specs.get(id).copied()
    }

    fn is_trunk(&self, id: &str) -> bool {
        self.spec(id).is_none_or(|spec| spec.trunk)
    }

    fn is_trigger(&self, id: &str) -> bool {
        self.spec(id).is_some_and(|spec| spec.trigger.is_some())
    }

    fn is_enabled(&self, id: &str) -> bool {
        self.nodes.get(id).is_some_and(|node| !node.is_disabled())
    }

    fn missing(&self, ids: [&str; 2], issues: &mut Issues) -> bool {
        let mut missing = false;
        for id in ids {
            if !self.has(id) {
                issues.node(ScenarioIssueRule::UnknownNode, id);
                missing = true;
            }
        }
        missing
    }
}

pub fn validate_graph(
    graph: &ScenarioGraphDto,
    catalogue: &ScenarioCatalogue,
) -> Vec<ScenarioIssueDto> {
    let mut issues = Issues::default();
    let nodes = Graph::new(graph, catalogue, &mut issues);
    let main = main_edges(graph, &nodes, &mut issues);
    let attachments = attachments(graph, &nodes, &main, &mut issues);
    check_slots(&nodes, &attachments, &mut issues);
    check_cycles(&nodes, &main, &mut issues);
    check_params(&nodes, &attachments, &mut issues);
    let triggers = check_triggers(&nodes, &mut issues);
    check_reachability(&nodes, &main, &attachments, &triggers, &mut issues);
    issues.0
}

pub fn trigger_binding(
    graph: &ScenarioGraphDto,
    catalogue: &ScenarioCatalogue,
) -> Option<ScenarioTriggerDto> {
    let mut seen = HashSet::new();
    graph
        .nodes
        .iter()
        .filter(|node| seen.insert(node.id.as_str()))
        .find_map(|node| {
            let kind = catalogue.spec(&node.node_type)?.trigger?;
            Some(ScenarioTriggerDto {
                kind,
                node_id: node.id.clone(),
                params: node.params.clone(),
            })
        })
}

fn main_edges<'g>(
    graph: &'g ScenarioGraphDto,
    nodes: &Graph<'g>,
    issues: &mut Issues,
) -> Vec<(&'g str, &'g str)> {
    let mut seen = HashSet::new();
    let mut main = Vec::new();
    for edge in &graph.edges {
        let (from, to) = (edge.from.as_str(), edge.to.as_str());
        if nodes.missing([from, to], issues) {
            continue;
        }
        if !seen.insert(edge) {
            issues.node(ScenarioIssueRule::DuplicateEdge, from);
            continue;
        }
        if !nodes.is_trunk(from) {
            issues.node(ScenarioIssueRule::InvalidEdge, from);
            continue;
        }
        if !nodes.is_trunk(to) || nodes.is_trigger(to) {
            issues.node(ScenarioIssueRule::InvalidEdge, to);
            continue;
        }
        main.push((from, to));
    }
    main
}

fn attachments<'g>(
    graph: &'g ScenarioGraphDto,
    nodes: &Graph<'g>,
    main: &[(&str, &str)],
    issues: &mut Issues,
) -> Vec<&'g ScenarioAttachmentDto> {
    let mut accepted: Vec<&ScenarioAttachmentDto> = Vec::new();
    for attachment in &graph.attachments {
        let (parent, child) = (
            attachment.parent_node_id.as_str(),
            attachment.child_node_id.as_str(),
        );
        if nodes.missing([parent, child], issues) {
            continue;
        }
        if parent == child {
            issues.node(ScenarioIssueRule::Cycle, child);
            continue;
        }
        if nodes
            .spec(parent)
            .is_some_and(|spec| spec.slot(attachment.slot).is_none())
        {
            issues.push(ScenarioIssueDto::slot(
                ScenarioIssueRule::UnsupportedSlot,
                parent,
                attachment.slot,
            ));
            continue;
        }
        if nodes
            .spec(child)
            .is_some_and(|spec| !spec.fills.contains(&attachment.slot))
        {
            issues.push(ScenarioIssueDto::slot(
                ScenarioIssueRule::SlotMismatch,
                child,
                attachment.slot,
            ));
            continue;
        }
        if accepted
            .iter()
            .any(|other| other.child_node_id == attachment.child_node_id)
        {
            issues.node(ScenarioIssueRule::MultipleParents, child);
            continue;
        }
        if main.iter().any(|(from, to)| *from == child || *to == child) {
            issues.node(ScenarioIssueRule::InvalidEdge, child);
        }
        accepted.push(attachment);
    }
    let parent_of: HashMap<&str, &str> = accepted
        .iter()
        .map(|attachment| {
            (
                attachment.child_node_id.as_str(),
                attachment.parent_node_id.as_str(),
            )
        })
        .collect();
    for node in &nodes.order {
        let mut current = node.id.as_str();
        for _ in 0..parent_of.len() {
            let Some(parent) = parent_of.get(current) else {
                break;
            };
            if *parent == node.id {
                issues.node(ScenarioIssueRule::Cycle, &node.id);
                break;
            }
            current = parent;
        }
    }
    accepted
}

fn check_slots(nodes: &Graph<'_>, attachments: &[&ScenarioAttachmentDto], issues: &mut Issues) {
    for node in &nodes.order {
        let Some(spec) = nodes.spec(&node.id) else {
            continue;
        };
        if node.is_disabled() {
            continue;
        }
        for slot in &spec.slots {
            let filled = attachments
                .iter()
                .filter(|attachment| {
                    attachment.parent_node_id == node.id
                        && attachment.slot == slot.slot
                        && nodes.is_enabled(&attachment.child_node_id)
                })
                .count();
            if slot.required && filled == 0 {
                issues.push(ScenarioIssueDto::slot(
                    ScenarioIssueRule::MissingAttachment,
                    &node.id,
                    slot.slot,
                ));
            }
            if !slot.multiple && filled > 1 {
                issues.push(ScenarioIssueDto::slot(
                    ScenarioIssueRule::TooManyAttachments,
                    &node.id,
                    slot.slot,
                ));
            }
        }
    }
}

struct Tarjan<'g> {
    adjacency: HashMap<&'g str, Vec<&'g str>>,
    index: HashMap<&'g str, usize>,
    low: HashMap<&'g str, usize>,
    stack: Vec<&'g str>,
    on_stack: HashSet<&'g str>,
    cyclic: HashSet<&'g str>,
}

impl<'g> Tarjan<'g> {
    fn visit(&mut self, node: &'g str) {
        let index = self.index.len();
        self.index.insert(node, index);
        self.low.insert(node, index);
        self.stack.push(node);
        self.on_stack.insert(node);
        let next = self.adjacency.get(node).cloned().unwrap_or_default();
        for target in next {
            if !self.index.contains_key(target) {
                self.visit(target);
                let low = self.low[node].min(self.low[target]);
                self.low.insert(node, low);
            } else if self.on_stack.contains(target) {
                let low = self.low[node].min(self.index[target]);
                self.low.insert(node, low);
            }
        }
        if self.low[node] == self.index[node] {
            let mut component = Vec::new();
            while let Some(member) = self.stack.pop() {
                self.on_stack.remove(member);
                component.push(member);
                if member == node {
                    break;
                }
            }
            let looped = self
                .adjacency
                .get(node)
                .is_some_and(|next| next.contains(&node));
            if component.len() > 1 || looped {
                self.cyclic.extend(component);
            }
        }
    }
}

fn check_cycles<'g>(nodes: &Graph<'g>, main: &[(&'g str, &'g str)], issues: &mut Issues) {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in main {
        adjacency.entry(from).or_default().push(to);
    }
    let mut tarjan = Tarjan {
        adjacency,
        index: HashMap::new(),
        low: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        cyclic: HashSet::new(),
    };
    for node in &nodes.order {
        if !tarjan.index.contains_key(node.id.as_str()) {
            tarjan.visit(&node.id);
        }
    }
    for node in &nodes.order {
        if tarjan.cyclic.contains(node.id.as_str()) {
            issues.node(ScenarioIssueRule::Cycle, &node.id);
        }
    }
}

fn is_missing(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => js_trim(text).is_empty(),
        Some(_) => false,
    }
}

fn check_params(nodes: &Graph<'_>, attachments: &[&ScenarioAttachmentDto], issues: &mut Issues) {
    let tools: HashSet<&str> = attachments
        .iter()
        .filter(|attachment| attachment.slot == ScenarioSlot::Tool)
        .map(|attachment| attachment.child_node_id.as_str())
        .collect();
    for node in &nodes.order {
        let Some(spec) = nodes.spec(&node.id) else {
            continue;
        };
        if node.is_disabled() || tools.contains(node.id.as_str()) {
            continue;
        }
        for param in &spec.required_params {
            if is_missing(node.params.get(param)) {
                issues.push(ScenarioIssueDto::param(&node.id, param));
            }
        }
    }
}

fn check_triggers<'g>(nodes: &Graph<'g>, issues: &mut Issues) -> Vec<&'g str> {
    let triggers: Vec<&str> = nodes
        .order
        .iter()
        .filter(|node| nodes.is_trigger(&node.id))
        .map(|node| node.id.as_str())
        .collect();
    if triggers.is_empty() {
        issues.push(ScenarioIssueDto::graph(ScenarioIssueRule::NoTrigger));
    }
    for extra in triggers.iter().skip(1) {
        issues.node(ScenarioIssueRule::MultipleTriggers, extra);
    }
    triggers
}

fn check_reachability<'g>(
    nodes: &Graph<'g>,
    main: &[(&'g str, &'g str)],
    attachments: &[&'g ScenarioAttachmentDto],
    triggers: &[&'g str],
    issues: &mut Issues,
) {
    if triggers.is_empty() {
        return;
    }
    let mut next: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in main {
        next.entry(from).or_default().push(to);
    }
    for attachment in attachments {
        next.entry(attachment.parent_node_id.as_str())
            .or_default()
            .push(attachment.child_node_id.as_str());
    }
    let mut reached: HashSet<&str> = triggers.iter().copied().collect();
    let mut queue: VecDeque<&str> = triggers.iter().copied().collect();
    while let Some(node) = queue.pop_front() {
        for target in next.get(node).into_iter().flatten() {
            if reached.insert(target) {
                queue.push_back(target);
            }
        }
    }
    for node in &nodes.order {
        if !reached.contains(node.id.as_str()) {
            issues.node(ScenarioIssueRule::Unreachable, &node.id);
        }
    }
}
