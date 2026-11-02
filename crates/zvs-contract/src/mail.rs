use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::ids::{ConversationId, RunId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const MAIL_APPROVAL_KIND: &str = "mail";
pub const MAIL_FOLDER_MAX: usize = 1000;
pub const MAIL_ADDRESS_MAX: usize = 320;
pub const MAIL_HEADER_MAX: usize = 2000;
pub const MAIL_SUBJECT_MAX: usize = 998;
pub const MAIL_SNIPPET_MAX: usize = 300;
pub const MAIL_TEXT_MAX: usize = 200_000;
pub const MAIL_RECIPIENTS_MAX: usize = 50;
pub const MAIL_REFERENCES_MAX: usize = 100;
pub const MAIL_ATTACHMENTS_MAX: usize = 100;
pub const MAIL_ACTION_MAX: usize = 2000;

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UntrustedTextDto {
    pub text: String,
    pub untrusted: bool,
}

impl UntrustedTextDto {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            untrusted: true,
        }
    }
}

impl Validate for UntrustedTextDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("text", &self.text, MAIL_TEXT_MAX)?;
        check::literal_true("untrusted", self.untrusted)
    }
}

fn check_untrusted(field: &str, value: &UntrustedTextDto, max: usize) -> Result<(), AppError> {
    nested(field, value.validate())?;
    check::max_len(&format!("{field}.text"), &value.text, max)
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailRefDto {
    pub folder: String,
    pub uid_validity: u32,
    pub uid: u32,
}

impl Validate for MailRefDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("folder", &self.folder)?;
        check::max_len("folder", &self.folder, MAIL_FOLDER_MAX)?;
        if self.uid == 0 {
            return Err(invalid("uid", "min"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailAttachmentDto {
    pub name: UntrustedTextDto,
    pub content_type: String,
    pub size: u64,
}

impl Validate for MailAttachmentDto {
    fn validate(&self) -> Result<(), AppError> {
        check_untrusted("name", &self.name, MAIL_HEADER_MAX)?;
        check::max_len("contentType", &self.content_type, MAIL_HEADER_MAX)?;
        check::safe_integer("size", self.size)
    }
}

fn check_list(field: &str, values: &[String], items: usize, max: usize) -> Result<(), AppError> {
    if values.len() > items {
        return Err(invalid(field, "maxItems"));
    }
    for (index, value) in values.iter().enumerate() {
        let path = format!("{field}.{index}");
        check::non_empty(&path, value)?;
        check::max_len(&path, value, max)?;
    }
    Ok(())
}

fn check_optional(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| {
        check::non_empty(field, value)?;
        check::max_len(field, value, max)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMessageDto {
    pub untrusted: bool,
    #[serde(rename = "ref")]
    pub reference: MailRefDto,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub message_id: Option<String>,
    pub from: UntrustedTextDto,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub from_address: Option<String>,
    pub to: Vec<String>,
    pub subject: UntrustedTextDto,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub date: Option<Timestamp>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub snippet: UntrustedTextDto,
    pub attachments: Vec<MailAttachmentDto>,
    pub size: u64,
    pub seen: bool,
}

impl Validate for MailMessageDto {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("untrusted", self.untrusted)?;
        nested("ref", self.reference.validate())?;
        check_optional("messageId", self.message_id.as_ref(), MAIL_HEADER_MAX)?;
        check_untrusted("from", &self.from, MAIL_HEADER_MAX)?;
        check_optional("fromAddress", self.from_address.as_ref(), MAIL_ADDRESS_MAX)?;
        check_list("to", &self.to, MAIL_RECIPIENTS_MAX, MAIL_ADDRESS_MAX)?;
        check_untrusted("subject", &self.subject, MAIL_HEADER_MAX)?;
        self.date
            .map_or(Ok(()), |date| check::timestamp("date", date))?;
        check_optional("inReplyTo", self.in_reply_to.as_ref(), MAIL_HEADER_MAX)?;
        check_list(
            "references",
            &self.references,
            MAIL_REFERENCES_MAX,
            MAIL_HEADER_MAX,
        )?;
        check_untrusted("snippet", &self.snippet, MAIL_SNIPPET_MAX)?;
        if self.attachments.len() > MAIL_ATTACHMENTS_MAX {
            return Err(invalid("attachments", "maxItems"));
        }
        nested("attachments", self.attachments.validate())?;
        check::safe_integer("size", self.size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailBodyDto {
    pub untrusted: bool,
    pub message: MailMessageDto,
    pub text: UntrustedTextDto,
    pub truncated: bool,
}

impl Validate for MailBodyDto {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("untrusted", self.untrusted)?;
        nested("message", self.message.validate())?;
        check_untrusted("text", &self.text, MAIL_TEXT_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailDraftDto {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub text: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

impl Validate for MailDraftDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("from", &self.from)?;
        check::max_len("from", &self.from, MAIL_HEADER_MAX)?;
        if self.to.is_empty() {
            return Err(invalid("to", "minItems"));
        }
        check_list("to", &self.to, MAIL_RECIPIENTS_MAX, MAIL_ADDRESS_MAX)?;
        check_list("cc", &self.cc, MAIL_RECIPIENTS_MAX, MAIL_ADDRESS_MAX)?;
        check::max_len("subject", &self.subject, MAIL_SUBJECT_MAX)?;
        check::max_len("text", &self.text, MAIL_TEXT_MAX)?;
        check_optional("inReplyTo", self.in_reply_to.as_ref(), MAIL_HEADER_MAX)?;
        check_list(
            "references",
            &self.references,
            MAIL_REFERENCES_MAX,
            MAIL_HEADER_MAX,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailApprovalDto {
    pub id: String,
    pub run_id: RunId,
    pub kind: String,
    pub subject: String,
    pub scope: String,
    pub expires_at: Timestamp,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<ConversationId>,
    pub tool: String,
    pub action: String,
    pub account: String,
    pub draft: MailDraftDto,
}

impl Validate for MailApprovalDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        if self.kind != MAIL_APPROVAL_KIND {
            return Err(invalid("kind", "literal"));
        }
        check::non_empty("subject", &self.subject)?;
        check::non_empty("tool", &self.tool)?;
        check::max_len("action", &self.action, MAIL_ACTION_MAX)?;
        check::non_empty("account", &self.account)?;
        check::max_len("account", &self.account, MAIL_HEADER_MAX)?;
        check::safe_integer("expiresAt", self.expires_at.unsigned_abs())?;
        nested("draft", self.draft.validate())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn untrusted_text_cannot_be_declared_trusted() {
        assert!(parse::<UntrustedTextDto>(json!({ "text": "x", "untrusted": true })).is_ok());
        assert!(parse::<UntrustedTextDto>(json!({ "text": "x", "untrusted": false })).is_err());
        assert!(parse::<UntrustedTextDto>(json!({ "text": "x" })).is_err());
    }

    #[test]
    fn a_draft_needs_a_recipient() {
        let draft = json!({
            "from": "me@example.com",
            "to": [],
            "cc": [],
            "subject": "Re: тест",
            "text": "Ответ",
            "inReplyTo": null,
            "references": [],
        });
        assert!(parse::<MailDraftDto>(draft).is_err());
    }
}
