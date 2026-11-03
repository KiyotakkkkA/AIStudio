use std::sync::Arc;
use std::time::Duration;

use zvs_contract::{ToolListFilter, ToolSource};

use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::services::SystemService;
use crate::services::chat::subagent::{AGENT_DELEGATE, delegate_tool};
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
        .filter(|tool| {
            tool.source == ToolSource::Builtin
                && model_tool(&tool.key)
                && tool.key != AGENT_DELEGATE
        })
        .map(|tool| tool.key.clone())
        .collect();
    let mut offered = offered_tools(&keys, &registered);
    offered.push(ask_tool(&offered));
    offered.push(delegate_tool(&offered));
    offered.push(memory_tool(&offered));
    let size = tool_specs_json(&offered).chars().count();
    println!(
        "chat tool catalogue: {} tools, {size} characters",
        offered.len()
    );
    let names: Vec<&str> = offered.iter().map(|tool| tool.key.as_str()).collect();
    let shell = offered
        .iter()
        .find(|tool| tool.key == "shell.run")
        .expect("shell tool");
    assert_eq!(
        crate::services::chat::process_tools_section(&offered),
        format!(
            "{}: {}\n{}",
            crate::services::chat::PROCESS_TOOLS_HEADING,
            shell.name,
            crate::services::chat::PROCESS_TOOLS_RULES
        )
    );
    assert!(crate::services::chat::process_tools_section(&[]).is_empty());
    assert_eq!(
        names,
        [
            "fs.apply",
            "fs.list",
            "fs.read",
            "fs.search",
            "knowledge.search",
            "shell.kill",
            "shell.output",
            "shell.run",
            "skill.load",
            "skill.save",
            "chat.ask",
            "agent.delegate",
            "chat.memory"
        ]
    );
    assert!(size < 16_000, "{size}");
}

#[test]
fn retired_code_tool_is_withdrawn_on_registration() {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    host.tools
        .register(&crate::tools::ToolDescriptor::new(
            "code.run",
            ToolSource::Builtin,
        ))
        .expect("seeds an old installation");
    crate::tools::register_kernel_tools(&host.nodes, &host.tools).expect("registers current tools");
    assert!(
        !host
            .tools
            .find_by_key("code.run")
            .expect("looks up")
            .expect("keeps history")
            .available
    );
    assert!(
        !host
            .tools
            .list(&ToolListFilter::default())
            .expect("lists")
            .iter()
            .any(|tool| tool.key == "code.run")
    );
    assert!(
        !crate::tools::current_tool_keys(&["code.run".to_owned(), "shell.run".to_owned()])
            .contains(&"code.run".to_owned())
    );
    assert!(host.nodes.get("code.run").is_err());
}
