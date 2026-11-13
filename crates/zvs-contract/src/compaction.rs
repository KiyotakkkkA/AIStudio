use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ConversationId, MessageId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, js_trim};

pub const COMPACT_INSTRUCTIONS_MAX: usize = 1000;
pub const COMPACT_SUMMARY_MAX: usize = 60_000;
pub const COMPACT_MODEL_ID_MAX: usize = 200;
pub const RECAP_MAX: usize = 4000;
pub const COMPACTION_STEP_DOMAIN: &str = "compaction";
pub const RECAP_STEP_DOMAIN: &str = "recap";

wire_enum!(CompactionTrigger {
    Manual => "manual",
    Auto => "auto",
});

wire_enum!(CompactionPhase {
    Started => "started",
    Finished => "finished",
    Failed => "failed",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCompactionDto {
    pub id: String,
    pub boundary_message_id: MessageId,
    pub summary: String,
    pub messages: u64,
    pub tokens_before: u64,
    pub tokens_after: u64,
    pub provider_id: String,
    pub model_id: String,
    pub trigger: CompactionTrigger,
    pub created_at: Timestamp,
}

impl Validate for ChatCompactionDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        check::non_empty("summary", js_trim(&self.summary))?;
        check::max_len("summary", &self.summary, COMPACT_SUMMARY_MAX)?;
        check::safe_integer("messages", self.messages)?;
        check::int_range("messages", self.messages, 1, u64::MAX)?;
        check::safe_integer("tokensBefore", self.tokens_before)?;
        check::safe_integer("tokensAfter", self.tokens_after)?;
        check::non_empty("providerId", &self.provider_id)?;
        check::max_len("providerId", &self.provider_id, COMPACT_MODEL_ID_MAX)?;
        check::non_empty("modelId", &self.model_id)?;
        check::max_len("modelId", &self.model_id, COMPACT_MODEL_ID_MAX)?;
        check::safe_integer("createdAt", self.created_at.unsigned_abs())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCompactInput {
    pub conversation_id: ConversationId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub instructions: Option<String>,
}

impl ChatCompactInput {
    pub fn focus(&self) -> Option<&str> {
        self.instructions
            .as_deref()
            .map(js_trim)
            .filter(|text| !text.is_empty())
    }
}

impl Validate for ChatCompactInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(instructions) = &self.instructions {
            check::max_len("instructions", instructions, COMPACT_INSTRUCTIONS_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCompactionRef {
    pub conversation_id: ConversationId,
    pub compaction_id: String,
}

impl Validate for ChatCompactionRef {
    fn validate(&self) -> Result<(), AppError> {
        if is_wire_uuid(&self.compaction_id) {
            Ok(())
        } else {
            Err(invalid("compactionId", "uuid"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCompactionStepDto {
    pub domain: String,
    pub conversation_id: ConversationId,
    pub trigger: CompactionTrigger,
    pub phase: CompactionPhase,
}

impl ChatCompactionStepDto {
    pub fn of(
        conversation_id: ConversationId,
        trigger: CompactionTrigger,
        phase: CompactionPhase,
    ) -> Self {
        Self {
            domain: COMPACTION_STEP_DOMAIN.to_owned(),
            conversation_id,
            trigger,
            phase,
        }
    }
}

impl Validate for ChatCompactionStepDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain == COMPACTION_STEP_DOMAIN {
            Ok(())
        } else {
            Err(invalid("domain", "literal"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRecapStepDto {
    pub domain: String,
    pub conversation_id: ConversationId,
    pub message_id: MessageId,
    pub recap: String,
}

impl ChatRecapStepDto {
    pub fn of(conversation_id: ConversationId, message_id: MessageId, recap: String) -> Self {
        Self {
            domain: RECAP_STEP_DOMAIN.to_owned(),
            conversation_id,
            message_id,
            recap,
        }
    }
}

pub fn check_recap(recap: &str) -> Result<(), AppError> {
    check::non_empty("recap", js_trim(recap))?;
    check::max_len("recap", recap, RECAP_MAX)
}

impl Validate for ChatRecapStepDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain != RECAP_STEP_DOMAIN {
            return Err(invalid("domain", "literal"));
        }
        check_recap(&self.recap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_focus_counts_as_none_and_a_long_one_is_refused() {
        let id = ConversationId::parse("0199aa11-1111-7111-8111-111111111111").expect("id");
        let blank = ChatCompactInput {
            conversation_id: id,
            instructions: Some("   ".to_owned()),
        };
        assert_eq!(blank.focus(), None);
        assert!(blank.validate().is_ok());
        let long = ChatCompactInput {
            conversation_id: id,
            instructions: Some("x".repeat(COMPACT_INSTRUCTIONS_MAX + 1)),
        };
        assert!(long.validate().is_err());
    }
}
