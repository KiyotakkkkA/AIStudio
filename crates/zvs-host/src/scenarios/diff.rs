use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use zvs_contract::{ScenarioDiffDto, ScenarioGraphDto, ScenarioNodeDto};

fn node_changed(before: &ScenarioNodeDto, after: &ScenarioNodeDto) -> bool {
    before.node_type != after.node_type
        || before.name != after.name
        || before.params != after.params
        || before.is_disabled() != after.is_disabled()
}

fn set_changes<T: Eq + Hash>(before: &[T], after: &[T]) -> (u64, u64) {
    let before: HashSet<&T> = before.iter().collect();
    let after: HashSet<&T> = after.iter().collect();
    (
        after.difference(&before).count() as u64,
        before.difference(&after).count() as u64,
    )
}

pub fn diff_graphs(before: &ScenarioGraphDto, after: &ScenarioGraphDto) -> ScenarioDiffDto {
    let old: HashMap<&str, &ScenarioNodeDto> = before
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let new: HashMap<&str, &ScenarioNodeDto> = after
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut summary = ScenarioDiffDto::default();
    let mut seen = HashSet::new();
    for node in &after.nodes {
        if !seen.insert(node.id.as_str()) {
            continue;
        }
        match old.get(node.id.as_str()) {
            None => summary.nodes_added.push(node.id.clone()),
            Some(previous) if node_changed(previous, node) => {
                summary.nodes_changed.push(node.id.clone());
            }
            Some(_) => {}
        }
    }
    seen.clear();
    for node in &before.nodes {
        if seen.insert(node.id.as_str()) && !new.contains_key(node.id.as_str()) {
            summary.nodes_removed.push(node.id.clone());
        }
    }
    (summary.edges_added, summary.edges_removed) = set_changes(&before.edges, &after.edges);
    (summary.attachments_added, summary.attachments_removed) =
        set_changes(&before.attachments, &after.attachments);
    summary
}
