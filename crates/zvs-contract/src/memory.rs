use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ConversationId, MemoryId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, no_refinements};

pub const MEMORY_CONTENT_MAX: usize = 500;
pub const MEMORY_ACCOUNT_MAX: usize = 100;
pub const MEMORY_CONVERSATION_MAX: usize = 50;

wire_enum!(MemoryScope {
    Account => "account",
    Conversation => "conversation",
});

wire_enum!(MemorySource {
    User => "user",
    Model => "model",
});

pub fn check_memory_content(field: &str, value: &str) -> Result<(), AppError> {
    check::trimmed_name(field, value, MEMORY_CONTENT_MAX)
}

fn check_scope(scope: MemoryScope, conversation: bool) -> Result<(), AppError> {
    if (scope == MemoryScope::Conversation) == conversation {
        Ok(())
    } else {
        Err(invalid("conversationId", "scope"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryChatDto {
    pub id: ConversationId,
    pub title: String,
}

no_refinements!(MemoryChatDto);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDto {
    pub id: MemoryId,
    pub scope: MemoryScope,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub conversation: Option<MemoryChatDto>,
    pub content: String,
    pub source: MemorySource,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for MemoryDto {
    fn validate(&self) -> Result<(), AppError> {
        if (self.scope == MemoryScope::Conversation) != self.conversation.is_some() {
            return Err(invalid("conversation", "scope"));
        }
        check_memory_content("content", &self.content)?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

pub type MemoryListOutput = Vec<MemoryDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMemoryInput {
    pub scope: MemoryScope,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<ConversationId>,
    pub content: String,
}

impl Validate for CreateMemoryInput {
    fn validate(&self) -> Result<(), AppError> {
        check_scope(self.scope, self.conversation_id.is_some())?;
        check_memory_content("content", &self.content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemoryInput {
    pub id: MemoryId,
    pub content: String,
}

impl Validate for UpdateMemoryInput {
    fn validate(&self) -> Result<(), AppError> {
        check_memory_content("content", &self.content)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRef {
    pub id: MemoryId,
}

no_refinements!(MemoryRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearMemoryInput {
    pub scope: MemoryScope,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<ConversationId>,
}

impl Validate for ClearMemoryInput {
    fn validate(&self) -> Result<(), AppError> {
        check_scope(self.scope, self.conversation_id.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_fact_needs_its_dialog_and_an_account_fact_has_none() {
        let chat = ConversationId::parse("0199aa11-1111-7111-8111-111111111111");
        let input = |scope, conversation_id| CreateMemoryInput {
            scope,
            conversation_id,
            content: "Пишет на Rust".to_owned(),
        };
        assert!(input(MemoryScope::Account, None).validate().is_ok());
        assert!(input(MemoryScope::Conversation, chat).validate().is_ok());
        assert!(input(MemoryScope::Account, chat).validate().is_err());
        assert!(input(MemoryScope::Conversation, None).validate().is_err());
    }

    #[test]
    fn content_is_trimmed_before_the_length_rules() {
        assert!(check_memory_content("content", "   ").is_err());
        let longest = "я".repeat(MEMORY_CONTENT_MAX);
        assert!(check_memory_content("content", &format!("  {longest}  ")).is_ok());
        assert!(check_memory_content("content", &format!("{longest}я")).is_err());
    }
}
