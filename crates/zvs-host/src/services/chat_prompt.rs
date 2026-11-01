use zvs_contract::{
    ChatHistoryEntryDto, ChatHistoryStatus, ChatPromptSection, MessageDto, MessageRole,
};

use super::chat_history::{MESSAGE_OVERHEAD, tokens, with_selections};
use crate::drivers::ai::{ChatMessage, ChatRole};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptParts {
    pub dialog: String,
    pub profile: String,
    pub memory: String,
    pub knowledge: String,
    pub mcp_guidance: String,
    pub command: String,
    pub skills: String,
    pub workspace: String,
    pub file_tools: String,
    pub process_tools: String,
    pub ended_shells: String,
    pub ask: String,
    pub delegate: String,
    pub deferred: String,
    pub goal: String,
}

impl PromptParts {
    pub fn sections(&self) -> [(ChatPromptSection, &str); 15] {
        [
            (ChatPromptSection::DialogPrompt, &self.dialog),
            (ChatPromptSection::Profile, &self.profile),
            (ChatPromptSection::Memory, &self.memory),
            (ChatPromptSection::Knowledge, &self.knowledge),
            (ChatPromptSection::McpGuidance, &self.mcp_guidance),
            (ChatPromptSection::Command, &self.command),
            (ChatPromptSection::Skills, &self.skills),
            (ChatPromptSection::Workspace, &self.workspace),
            (ChatPromptSection::FileTools, &self.file_tools),
            (ChatPromptSection::ProcessTools, &self.process_tools),
            (ChatPromptSection::EndedShells, &self.ended_shells),
            (ChatPromptSection::Ask, &self.ask),
            (ChatPromptSection::Delegate, &self.delegate),
            (ChatPromptSection::Deferred, &self.deferred),
            (ChatPromptSection::Goal, &self.goal),
        ]
    }

    pub fn compose(&self) -> String {
        join(self.sections().into_iter().map(|(_, text)| text))
    }

    pub fn compose_where(&self, keep: impl Fn(ChatPromptSection) -> bool) -> String {
        join(
            self.sections()
                .into_iter()
                .filter(|(section, _)| keep(*section))
                .map(|(_, text)| text),
        )
    }
}

fn join<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub const fn in_tool_block(section: ChatPromptSection) -> bool {
    matches!(
        section,
        ChatPromptSection::McpGuidance
            | ChatPromptSection::Command
            | ChatPromptSection::Skills
            | ChatPromptSection::Workspace
            | ChatPromptSection::FileTools
            | ChatPromptSection::ProcessTools
            | ChatPromptSection::Ask
            | ChatPromptSection::Delegate
            | ChatPromptSection::Deferred
            | ChatPromptSection::ToolSpecs
    )
}

pub fn sendable(message: &MessageDto) -> bool {
    message.role != MessageRole::Assistant || !message.content.trim().is_empty()
}

const fn chat_role(role: MessageRole) -> ChatRole {
    match role {
        MessageRole::User | MessageRole::Event => ChatRole::User,
        MessageRole::Assistant => ChatRole::Assistant,
    }
}

pub fn model_message(message: &MessageDto) -> ChatMessage {
    ChatMessage::new(
        chat_role(message.role),
        with_selections(&message.content, &message.selections),
    )
}

pub fn model_history(messages: &[MessageDto]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter(|message| sendable(message))
        .map(model_message)
        .collect()
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

pub fn history_entries(messages: &[MessageDto], trimmed: usize) -> Vec<ChatHistoryEntryDto> {
    let mut remaining = trimmed;
    messages
        .iter()
        .map(|message| {
            let text = with_selections(&message.content, &message.selections);
            let status = if !sendable(message) {
                ChatHistoryStatus::Skipped
            } else if remaining > 0 {
                remaining -= 1;
                ChatHistoryStatus::Trimmed
            } else {
                ChatHistoryStatus::Included
            };
            let tool_tokens: i64 = message
                .tool_calls
                .iter()
                .map(|call| {
                    tokens(&call.arguments.to_string()) + call.result.as_deref().map_or(0, tokens)
                })
                .sum();
            ChatHistoryEntryDto {
                message_id: message.id,
                role: message.role,
                status,
                tokens: count(tokens(&text) + MESSAGE_OVERHEAD),
                text,
                reasoning_tokens: count(tokens(&message.reasoning)),
                tool_tokens: count(tool_tokens),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(id: &str, role: &str, content: &str) -> MessageDto {
        serde_json::from_value(json!({
            "id": id,
            "conversationId": "0199aa11-1111-7111-8111-111111111111",
            "role": role,
            "content": content,
            "citations": [],
            "tokensIn": 0,
            "tokensOut": 0,
            "usageEstimated": true,
            "durationMs": 0,
            "runId": null,
            "partial": false,
            "createdAt": 1,
        }))
        .expect("a message")
    }

    const IDS: [&str; 4] = [
        "0199aa11-2222-7222-8222-000000000001",
        "0199aa11-2222-7222-8222-000000000002",
        "0199aa11-2222-7222-8222-000000000003",
        "0199aa11-2222-7222-8222-000000000004",
    ];

    #[test]
    fn sections_join_in_a_fixed_order_and_skip_blank_parts() {
        let parts = PromptParts {
            dialog: "Dialog".to_owned(),
            memory: "  ".to_owned(),
            goal: "Goal".to_owned(),
            workspace: "Folder".to_owned(),
            ..PromptParts::default()
        };
        assert_eq!(parts.compose(), "Dialog\n\nFolder\n\nGoal");
        assert_eq!(
            parts.compose_where(|section| !in_tool_block(section)),
            "Dialog\n\nGoal"
        );
    }

    #[test]
    fn an_empty_answer_is_left_out_of_the_history() {
        let messages = [
            message(IDS[0], "user", "Вопрос"),
            message(IDS[1], "assistant", "  "),
            message(IDS[2], "user", "Повтор"),
            message(IDS[3], "assistant", "Ответ"),
        ];
        assert_eq!(
            model_history(&messages),
            [
                ChatMessage::user("Вопрос"),
                ChatMessage::user("Повтор"),
                ChatMessage::assistant("Ответ"),
            ]
        );
    }

    #[test]
    fn history_entries_mark_skipped_trimmed_and_included_messages() {
        let messages = [
            message(IDS[0], "user", "Вопрос"),
            message(IDS[1], "assistant", ""),
            message(IDS[2], "user", "Повтор"),
            message(IDS[3], "assistant", "Ответ"),
        ];
        let entries = history_entries(&messages, 2);
        assert_eq!(
            entries.iter().map(|entry| entry.status).collect::<Vec<_>>(),
            [
                ChatHistoryStatus::Trimmed,
                ChatHistoryStatus::Skipped,
                ChatHistoryStatus::Trimmed,
                ChatHistoryStatus::Included,
            ]
        );
        assert_eq!(entries[3].tokens, count(tokens("Ответ") + MESSAGE_OVERHEAD));
    }
}
