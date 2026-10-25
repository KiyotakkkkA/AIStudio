use std::sync::Arc;
use std::time::Duration;

use zvs_contract::{ToolListFilter, ToolSource};

use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::services::SystemService;
use crate::services::chat_ask::ask_tool;
use crate::services::chat_memory::memory_tool;
use crate::services::chat_tools::{offered_tools, tool_specs_json};
use crate::testing::test_services;
use crate::tools::model_tool;

#[test]
fn the_full_chat_tool_catalogue_is_measured() {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    crate::kernel::knowledge_nodes::register_knowledge_nodes(&host.nodes).expect("registers");
    crate::tools::register_kernel_tools(&host.nodes, &host.tools).expect("registers");
    let registered = host.tools.list(&ToolListFilter::default()).expect("lists");
    let keys: Vec<String> = registered
        .iter()
        .filter(|tool| tool.source == ToolSource::Builtin && model_tool(&tool.key))
        .map(|tool| tool.key.clone())
        .collect();
    let mut offered = offered_tools(&keys, &registered);
    offered.push(ask_tool(&offered));
    offered.push(memory_tool(&offered));
    let size = tool_specs_json(&offered).chars().count();
    println!(
        "chat tool catalogue: {} tools, {size} characters",
        offered.len()
    );
    let names: Vec<&str> = offered.iter().map(|tool| tool.key.as_str()).collect();
    assert_eq!(
        names,
        [
            "code.run",
            "fs.apply",
            "fs.list",
            "fs.read",
            "fs.search",
            "knowledge.search",
            "shell.run",
            "skill.load",
            "skill.save",
            "chat.ask",
            "chat.memory"
        ]
    );
    assert!(size < 15_000, "{size}");
}
