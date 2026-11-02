use serde::{Deserialize, Serialize};

use crate::chat::CHAT_TEXT_MAX;
use crate::errors::AppError;
use crate::ids::ConversationId;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

pub const QUEUE_STEP_DOMAIN: &str = "queue";
pub const CHAT_QUEUE_MAX: usize = 10;
pub const QUEUE_ID_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueuedMessageDto {
    pub id: String,
    pub text: String,
    pub created_at: Timestamp,
}

impl Validate for ChatQueuedMessageDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, QUEUE_ID_MAX)?;
        check::trimmed_name("text", &self.text, CHAT_TEXT_MAX)?;
        check::timestamp("createdAt", self.created_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueueDto {
    pub conversation_id: ConversationId,
    pub messages: Vec<ChatQueuedMessageDto>,
    pub urgent: bool,
}

impl ChatQueueDto {
    pub const fn empty(conversation_id: ConversationId) -> Self {
        Self {
            conversation_id,
            messages: Vec::new(),
            urgent: false,
        }
    }
}

impl Validate for ChatQueueDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.messages.len() > CHAT_QUEUE_MAX {
            return Err(invalid("messages", "maxItems"));
        }
        nested("messages", self.messages.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueueStepDto {
    pub domain: String,
    #[serde(flatten)]
    pub queue: ChatQueueDto,
}

impl ChatQueueStepDto {
    pub fn of(queue: ChatQueueDto) -> Self {
        Self {
            domain: QUEUE_STEP_DOMAIN.to_owned(),
            queue,
        }
    }
}

impl Validate for ChatQueueStepDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain != QUEUE_STEP_DOMAIN {
            return Err(invalid("domain", "literal"));
        }
        self.queue.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueueInput {
    pub conversation_id: ConversationId,
}

no_refinements!(ChatQueueInput);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueueAddInput {
    pub conversation_id: ConversationId,
    pub text: String,
}

impl ChatQueueAddInput {
    pub fn text(&self) -> &str {
        js_trim(&self.text)
    }
}

impl Validate for ChatQueueAddInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("text", &self.text, CHAT_TEXT_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueueEditInput {
    pub conversation_id: ConversationId,
    pub message_id: String,
    pub text: String,
}

impl ChatQueueEditInput {
    pub fn text(&self) -> &str {
        js_trim(&self.text)
    }
}

impl Validate for ChatQueueEditInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("messageId", &self.message_id)?;
        check::max_len("messageId", &self.message_id, QUEUE_ID_MAX)?;
        check::trimmed_name("text", &self.text, CHAT_TEXT_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueueRemoveInput {
    pub conversation_id: ConversationId,
    pub message_id: String,
}

impl Validate for ChatQueueRemoveInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("messageId", &self.message_id)?;
        check::max_len("messageId", &self.message_id, QUEUE_ID_MAX)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const CONVERSATION: &str = "0199aa11-2222-7111-8111-111111111111";

    #[test]
    fn a_queue_step_flattens_the_queue_beside_its_domain() {
        let step: ChatQueueStepDto = parse(json!({
            "domain": "queue",
            "conversationId": CONVERSATION,
            "messages": [{ "id": "q1", "text": "Ещё", "createdAt": 5 }],
            "urgent": true,
        }))
        .expect("a step");
        assert!(step.queue.urgent);
        assert_eq!(step.queue.messages[0].text, "Ещё");
        assert!(
            parse::<ChatQueueStepDto>(json!({
                "domain": "goal",
                "conversationId": CONVERSATION,
                "messages": [],
                "urgent": false,
            }))
            .is_err()
        );
    }

    #[test]
    fn a_queue_holds_at_most_ten_messages_and_never_a_blank_one() {
        let message = |text: &str| json!({ "id": "q", "text": text, "createdAt": 1 });
        let full: Vec<_> = (0..=CHAT_QUEUE_MAX).map(|_| message("x")).collect();
        assert!(
            parse::<ChatQueueDto>(json!({
                "conversationId": CONVERSATION, "messages": full, "urgent": false,
            }))
            .is_err()
        );
        assert!(
            parse::<ChatQueueAddInput>(json!({ "conversationId": CONVERSATION, "text": "  " }))
                .is_err()
        );
        let edit: ChatQueueEditInput = parse(json!({
            "conversationId": CONVERSATION, "messageId": "q", "text": " Да ",
        }))
        .expect("an edit");
        assert_eq!(edit.text(), "Да");
    }
}
