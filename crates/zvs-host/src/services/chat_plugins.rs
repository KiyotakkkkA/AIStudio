use zvs_contract::{AppError, AppErrorCode, ChatPluginDto, IntegrationId, MessageDto, MessageRole};

use super::chat_tools::OfferedTool;

pub const PLUGIN_HEADING: &str = "Instructions of the plugin";
pub const PLUGIN_TOOLS: &str = "Tools of this account";
pub const PLUGIN_UNAVAILABLE: &str = "Плагин недоступен: аккаунт удалён или отключён";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPluginInfo {
    pub plugin: ChatPluginDto,
    pub label: String,
    pub instructions: String,
}

pub trait ChatPlugins: Send + Sync {
    fn plugin(&self, account_id: &str) -> Option<ChatPluginInfo>;
}

pub fn resolve_plugins(
    plugins: Option<&dyn ChatPlugins>,
    ids: &[IntegrationId],
) -> Result<Vec<ChatPluginDto>, AppError> {
    ids.iter()
        .map(|id| {
            plugins
                .and_then(|plugins| plugins.plugin(&id.to_string()))
                .map(|info| info.plugin)
                .ok_or_else(|| {
                    AppError::new(AppErrorCode::ValidationFailed, PLUGIN_UNAVAILABLE)
                        .with_detail("field", "plugins")
                        .with_detail("accountId", id.to_string())
                })
        })
        .collect()
}

pub fn active_plugins(messages: &[MessageDto]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for plugin in messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .flat_map(|message| &message.plugins)
    {
        let id = plugin.account_id.to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

pub fn plugin_note(
    plugins: Option<&dyn ChatPlugins>,
    ids: &[String],
    offered: &[OfferedTool],
) -> String {
    let Some(plugins) = plugins else {
        return String::new();
    };
    ids.iter()
        .filter_map(|id| {
            let info = plugins.plugin(id)?;
            let tools: Vec<&str> = offered
                .iter()
                .filter(|tool| tool.plugin.as_deref() == Some(id.as_str()))
                .map(|tool| tool.name.as_str())
                .collect();
            if tools.is_empty() {
                return None;
            }
            Some(format!(
                "{PLUGIN_HEADING} {} · {}:\n{}\n{PLUGIN_TOOLS}: {}",
                info.label,
                info.plugin.name,
                info.instructions,
                tools.join(", ")
            ))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::{IntegrationKind, OfferedToolSource};

    use super::*;

    const ACCOUNT: &str = "0199aa11-1111-7111-8111-000000000001";
    const OTHER: &str = "0199aa11-1111-7111-8111-000000000002";

    struct Plugins;

    impl ChatPlugins for Plugins {
        fn plugin(&self, account_id: &str) -> Option<ChatPluginInfo> {
            (account_id == ACCOUNT).then(|| ChatPluginInfo {
                plugin: ChatPluginDto {
                    account_id: IntegrationId::parse(ACCOUNT).expect("an id"),
                    kind: IntegrationKind::Github,
                    name: "Работа".to_owned(),
                },
                label: "GitHub".to_owned(),
                instructions: "GitHub account.".to_owned(),
            })
        }
    }

    fn offered(name: &str, plugin: Option<&str>) -> OfferedTool {
        OfferedTool {
            key: name.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            parameters: json!({}),
            destructive: false,
            read_only: true,
            source: OfferedToolSource::Command,
            server: None,
            plugin: plugin.map(str::to_owned),
        }
    }

    #[test]
    fn attached_accounts_resolve_to_named_plugins_and_unknown_ones_are_refused() {
        let id = IntegrationId::parse(ACCOUNT).expect("an id");
        let resolved = resolve_plugins(Some(&Plugins), &[id]).expect("resolves");
        assert_eq!(resolved[0].name, "Работа");
        let other = IntegrationId::parse(OTHER).expect("an id");
        let refused = resolve_plugins(Some(&Plugins), &[other]).expect_err("refuses");
        assert_eq!(refused.message, PLUGIN_UNAVAILABLE);
        assert!(resolve_plugins(None, &[id]).is_err());
        assert!(
            resolve_plugins(None, &[])
                .expect("nothing to resolve")
                .is_empty()
        );
    }

    #[test]
    fn the_note_names_the_account_and_lists_only_its_offered_tools() {
        let tools = [
            offered("github__aaaa__readFile", Some(ACCOUNT)),
            offered("fs__read", None),
        ];
        let note = plugin_note(
            Some(&Plugins),
            &[ACCOUNT.to_owned(), OTHER.to_owned()],
            &tools,
        );
        assert_eq!(
            note,
            format!(
                "{PLUGIN_HEADING} GitHub · Работа:\nGitHub account.\n{PLUGIN_TOOLS}: github__aaaa__readFile"
            )
        );
        assert!(plugin_note(Some(&Plugins), &[ACCOUNT.to_owned()], &tools[1..]).is_empty());
    }
}
