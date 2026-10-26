use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use super::nodes::{SKILL_LOAD, SkillLoadOutput};
use crate::commands::SHELL_RUN;
use crate::files::FS_READ;
use crate::files::roots::SKILL_ROOT;
use crate::services::chat_tools::{OfferedTool, wire_names};

pub const READ_BEFORE_RUN: &str = "Сначала прочитайте файлы навыка";
pub const SKILL_LOAD_RESULT_MAX_CHARS: usize = 40_000;
pub const SKILL_LOAD_RESULT_SHARE: usize = 6;

#[derive(Debug, Default)]
pub struct SkillTurn {
    requires: BTreeMap<String, Vec<String>>,
    read: HashSet<String>,
}

fn skill_path(raw: &str) -> Option<String> {
    let rest = raw.trim().strip_prefix(SKILL_ROOT)?;
    let rest = rest.trim_start_matches('/').replace('\\', "/");
    (!rest.is_empty()).then_some(rest)
}

impl SkillTurn {
    pub fn observe(&mut self, tool_key: &str, arguments: &Value, output: &Value) {
        match tool_key {
            SKILL_LOAD => {
                let Ok(loaded) = serde_json::from_value::<SkillLoadOutput>(output.clone()) else {
                    return;
                };
                for path in &loaded.inlined {
                    self.read.insert(format!("{}/{path}", loaded.key));
                }
                for (script, needed) in loaded.requires {
                    self.requires.insert(
                        format!("{}/{script}", loaded.key),
                        needed
                            .iter()
                            .map(|path| format!("{}/{path}", loaded.key))
                            .collect(),
                    );
                }
            }
            FS_READ => {
                let paths = arguments
                    .get("paths")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten();
                for target in paths {
                    let path = target
                        .get("path")
                        .and_then(Value::as_str)
                        .or_else(|| target.as_str());
                    if let Some(path) = path.and_then(skill_path) {
                        self.read.insert(path);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn hint(&self, tool_key: &str, arguments: &Value) -> Option<String> {
        if tool_key != SHELL_RUN {
            return None;
        }
        let program = arguments
            .get("argv")
            .and_then(Value::as_array)
            .and_then(|argv| argv.first())
            .and_then(Value::as_str)?;
        let needed = self.requires.get(&skill_path(program)?)?;
        let missing: Vec<String> = needed
            .iter()
            .filter(|path| !self.read.contains(*path))
            .map(|path| format!("{SKILL_ROOT}{path}"))
            .collect();
        (!missing.is_empty()).then(|| format!("{READ_BEFORE_RUN}: {}", missing.join(", ")))
    }
}

pub fn result_limit(tool_key: &str, limit: usize) -> usize {
    if tool_key == SKILL_LOAD {
        limit
            .saturating_mul(SKILL_LOAD_RESULT_SHARE)
            .clamp(limit, SKILL_LOAD_RESULT_MAX_CHARS.max(limit))
    } else {
        limit
    }
}

pub fn presented<'a>(
    tool_key: &str,
    text: String,
    offered: impl IntoIterator<Item = &'a OfferedTool>,
) -> String {
    if tool_key == SKILL_LOAD {
        wire_names(&text, offered)
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn loaded() -> Value {
        json!({
            "key": "dashboard",
            "name": "Дашборды",
            "folder": "skill:dashboard/",
            "files": [],
            "inlined": ["references/exploration.md"],
            "required": ["references/spec.md"],
            "requires": { "scripts/build.py": ["references/spec.md", "references/exploration.md"] },
            "text": "…"
        })
    }

    fn run(program: &str) -> Value {
        json!({ "argv": [program, "_spec.json"] })
    }

    #[test]
    fn a_script_waits_for_the_files_it_requires_and_runs_once_they_were_read() {
        let mut turn = SkillTurn::default();
        assert_eq!(
            turn.hint(SHELL_RUN, &run("skill:dashboard/scripts/build.py")),
            None
        );
        turn.observe(SKILL_LOAD, &json!({ "key": "dashboard" }), &loaded());
        assert_eq!(
            turn.hint(SHELL_RUN, &run("skill:dashboard/scripts/build.py")),
            Some(format!(
                "{READ_BEFORE_RUN}: skill:dashboard/references/spec.md"
            ))
        );
        assert_eq!(
            turn.hint(SHELL_RUN, &run("skill:dashboard/scripts/check.py")),
            None
        );
        assert_eq!(
            turn.hint(FS_READ, &run("skill:dashboard/scripts/build.py")),
            None
        );
        turn.observe(
            FS_READ,
            &json!({ "paths": [{ "path": "skill:dashboard/references/spec.md", "startLine": 1 }] }),
            &json!({}),
        );
        assert_eq!(
            turn.hint(SHELL_RUN, &run(" skill:dashboard/scripts/build.py")),
            None
        );
    }

    #[test]
    fn a_loaded_skill_gets_a_larger_result_limit_and_the_offered_tool_names() {
        assert_eq!(result_limit("fs.read", 8000), 8000);
        assert_eq!(result_limit(SKILL_LOAD, 8000), SKILL_LOAD_RESULT_MAX_CHARS);
        assert_eq!(result_limit(SKILL_LOAD, 500), 3000);
        assert_eq!(result_limit(SKILL_LOAD, 50_000), 50_000);
        let offered = |name: &str| OfferedTool {
            key: "fs.read".to_owned(),
            name: name.to_owned(),
            description: String::new(),
            parameters: json!({}),
            destructive: false,
            read_only: true,
            source: zvs_contract::OfferedToolSource::Base,
            server: None,
        };
        assert_eq!(
            presented(
                SKILL_LOAD,
                "Use fs.read.".to_owned(),
                &[offered("fs__read")]
            ),
            "Use fs__read."
        );
        assert_eq!(
            presented("fs.read", "Use fs.read.".to_owned(), &[offered("fs__read")]),
            "Use fs.read."
        );
    }
}
