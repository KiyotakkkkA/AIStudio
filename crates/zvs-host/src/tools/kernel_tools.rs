use std::collections::BTreeMap;

use zvs_contract::{AppError, ToolDto, ToolSource, ToolTier};

use super::registry::{ToolDescriptor, ToolRegistry};
use crate::commands::nodes::{CODE_DESCRIPTION, CODE_RUN, SHELL_DESCRIPTION, SHELL_RUN};
use crate::files::nodes::{
    APPLY_DESCRIPTION as FS_APPLY_DESCRIPTION, FS_APPLY, FS_LIST, FS_READ, FS_SEARCH,
    LIST_DESCRIPTION as FS_LIST_DESCRIPTION, READ_DESCRIPTION as FS_READ_DESCRIPTION,
    SEARCH_DESCRIPTION as FS_SEARCH_DESCRIPTION,
};
use crate::kernel::knowledge_nodes::{KNOWLEDGE_SEARCH, SEARCH_DESCRIPTION};
use crate::kernel::{NodeRegistry, PermissionRequirement};
use crate::skills::nodes::{
    LOAD_DESCRIPTION as SKILL_LOAD_DESCRIPTION, SAVE_DESCRIPTION as SKILL_SAVE_DESCRIPTION,
    SKILL_LOAD, SKILL_SAVE,
};

pub const NOT_TOOLS: &[&str] = &["llm.generate", "vector.index", "job.run", "download.fetch"];
const ASK_BY_DEFAULT: &[&str] = &[FS_APPLY];

struct BuiltinToolMeta {
    display_name: &'static str,
    description: &'static str,
    destructive: bool,
}

const BUILTIN: &[(&str, BuiltinToolMeta)] = &[
    (
        "vector.search",
        BuiltinToolMeta {
            display_name: "vector.search",
            description: "Семантический поиск по векторному хранилищу; возвращает фрагменты с оценками. storeId — UUID из vector.list.",
            destructive: false,
        },
    ),
    (
        "vector.list",
        BuiltinToolMeta {
            display_name: "vector.list",
            description: "Список векторных хранилищ: UUID, название, описание и число документов. UUID передаётся в vector.search как storeId.",
            destructive: false,
        },
    ),
    (
        KNOWLEDGE_SEARCH,
        BuiltinToolMeta {
            display_name: "knowledge.search",
            description: SEARCH_DESCRIPTION,
            destructive: false,
        },
    ),
    (
        SKILL_LOAD,
        BuiltinToolMeta {
            display_name: "skill.load",
            description: SKILL_LOAD_DESCRIPTION,
            destructive: false,
        },
    ),
    (
        SKILL_SAVE,
        BuiltinToolMeta {
            display_name: "skill.save",
            description: SKILL_SAVE_DESCRIPTION,
            destructive: true,
        },
    ),
    (
        FS_LIST,
        BuiltinToolMeta {
            display_name: "fs.list",
            description: FS_LIST_DESCRIPTION,
            destructive: false,
        },
    ),
    (
        FS_READ,
        BuiltinToolMeta {
            display_name: "fs.read",
            description: FS_READ_DESCRIPTION,
            destructive: false,
        },
    ),
    (
        FS_SEARCH,
        BuiltinToolMeta {
            display_name: "fs.search",
            description: FS_SEARCH_DESCRIPTION,
            destructive: false,
        },
    ),
    (
        FS_APPLY,
        BuiltinToolMeta {
            display_name: "fs.apply",
            description: FS_APPLY_DESCRIPTION,
            destructive: false,
        },
    ),
    (
        CODE_RUN,
        BuiltinToolMeta {
            display_name: "code.run",
            description: CODE_DESCRIPTION,
            destructive: true,
        },
    ),
    (
        SHELL_RUN,
        BuiltinToolMeta {
            display_name: "shell.run",
            description: SHELL_DESCRIPTION,
            destructive: true,
        },
    ),
];

pub const GRAPH_ONLY_TOOLS: &[&str] = &["vector.search", "vector.list"];

const RENAMED_TOOLS: &[(&str, &[&str])] = &[
    ("chat.file", &[FS_READ, FS_LIST, FS_APPLY]),
    ("skill.read", &[FS_READ]),
    ("skill.script", &[SHELL_RUN]),
    ("knowledge.read", &[KNOWLEDGE_SEARCH]),
    ("knowledge.list", &[KNOWLEDGE_SEARCH]),
    ("vector.index", &[]),
    ("job.run", &[]),
    ("download.fetch", &[]),
];

pub fn current_tool_keys(keys: &[String]) -> Vec<String> {
    let mut current: Vec<String> = Vec::with_capacity(keys.len());
    for key in keys {
        let renamed = RENAMED_TOOLS
            .iter()
            .find(|(old, _)| old == key)
            .map_or_else(
                || vec![key.clone()],
                |(_, new)| new.iter().map(|key| (*key).to_owned()).collect(),
            );
        for key in renamed {
            if !current.contains(&key) {
                current.push(key);
            }
        }
    }
    current
}

pub fn model_tool(key: &str) -> bool {
    !GRAPH_ONLY_TOOLS.contains(&key) && !NOT_TOOLS.contains(&key)
}

fn meta(key: &str) -> Option<&'static BuiltinToolMeta> {
    BUILTIN
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, meta)| meta)
}

pub fn register_kernel_tools(
    nodes: &NodeRegistry,
    tools: &ToolRegistry,
) -> Result<Vec<ToolDto>, AppError> {
    let mut registered: BTreeMap<String, ToolDto> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for node in nodes.list() {
        let PermissionRequirement::Tool { tool: key, .. } = node.permission() else {
            continue;
        };
        if NOT_TOOLS.contains(&key.as_str()) {
            continue;
        }
        let previous = registered.get(key).is_none_or(|dto| dto.read_only);
        let meta = meta(key);
        let dto = tools.register(&ToolDescriptor {
            source_ref: None,
            display_name: Some(
                meta.map_or_else(|| key.clone(), |meta| meta.display_name.to_owned()),
            ),
            description: meta.map_or_else(String::new, |meta| meta.description.to_owned()),
            input_schema: node.input_schema().clone(),
            read_only: node.side_effect_free() && previous,
            destructive: meta.is_some_and(|meta| meta.destructive),
            tier: (meta.is_some_and(|meta| meta.destructive)
                || ASK_BY_DEFAULT.contains(&key.as_str()))
            .then_some(ToolTier::Ask),
            ..ToolDescriptor::new(key.clone(), ToolSource::Builtin)
        })?;
        if !order.contains(key) {
            order.push(key.clone());
        }
        registered.insert(key.clone(), dto);
    }
    Ok(order
        .into_iter()
        .filter_map(|key| registered.remove(&key))
        .collect())
}
