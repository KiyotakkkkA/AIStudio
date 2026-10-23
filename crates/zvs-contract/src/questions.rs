use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ConversationId, RunId, is_wire_uuid};
use crate::validate::{Validate, check, invalid, nested};

pub const QUESTIONS_MAX: usize = 6;
pub const QUESTION_OPTIONS_MIN: usize = 2;
pub const QUESTION_OPTIONS_MAX: usize = 8;
pub const QUESTION_ID_MAX: usize = 32;
pub const QUESTION_TITLE_MAX: usize = 300;
pub const QUESTION_LABEL_MAX: usize = 200;
pub const QUESTION_DESCRIPTION_MAX: usize = 500;
pub const QUESTION_ANSWER_MAX: usize = 4000;

wire_enum!(QuestionKind {
    Text => "text",
    Single => "single",
    Multiple => "multiple",
});

fn check_id(field: &str, value: &str) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, QUESTION_ID_MAX)
}

fn check_request_id(field: &str, value: &str) -> Result<(), AppError> {
    if is_wire_uuid(value) {
        Ok(())
    } else {
        Err(invalid(field, "uuid"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOptionDto {
    pub id: String,
    pub label: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub description: Option<String>,
    pub recommended: bool,
}

impl Validate for QuestionOptionDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id("id", &self.id)?;
        check::trimmed_name("label", &self.label, QUESTION_LABEL_MAX)?;
        if let Some(description) = &self.description {
            check::max_len("description", description, QUESTION_DESCRIPTION_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionDto {
    pub id: String,
    pub title: String,
    pub kind: QuestionKind,
    pub options: Vec<QuestionOptionDto>,
    pub allow_other: bool,
}

impl Validate for QuestionDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id("id", &self.id)?;
        check::trimmed_name("title", &self.title, QUESTION_TITLE_MAX)?;
        if self.kind == QuestionKind::Text {
            if !self.options.is_empty() {
                return Err(invalid("options", "maxItems"));
            }
            if self.allow_other {
                return Err(invalid("allowOther", "kind"));
            }
            return Ok(());
        }
        if self.options.len() < QUESTION_OPTIONS_MIN {
            return Err(invalid("options", "minItems"));
        }
        if self.options.len() > QUESTION_OPTIONS_MAX {
            return Err(invalid("options", "maxItems"));
        }
        let mut ids = HashSet::new();
        for (index, option) in self.options.iter().enumerate() {
            nested(&format!("options.{index}"), option.validate())?;
            if !ids.insert(option.id.as_str()) {
                return Err(invalid(&format!("options.{index}.id"), "unique"));
            }
        }
        if self
            .options
            .iter()
            .filter(|option| option.recommended)
            .count()
            > 1
        {
            return Err(invalid("options", "recommended"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequestDto {
    pub id: String,
    pub run_id: RunId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<ConversationId>,
    pub questions: Vec<QuestionDto>,
}

impl Validate for QuestionRequestDto {
    fn validate(&self) -> Result<(), AppError> {
        check_request_id("id", &self.id)?;
        if self.questions.is_empty() {
            return Err(invalid("questions", "minItems"));
        }
        if self.questions.len() > QUESTIONS_MAX {
            return Err(invalid("questions", "maxItems"));
        }
        let mut ids = HashSet::new();
        for (index, question) in self.questions.iter().enumerate() {
            nested(&format!("questions.{index}"), question.validate())?;
            if !ids.insert(question.id.as_str()) {
                return Err(invalid(&format!("questions.{index}.id"), "unique"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswerDto {
    pub question_id: String,
    pub selected: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub text: Option<String>,
}

impl Validate for QuestionAnswerDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id("questionId", &self.question_id)?;
        if self.selected.len() > QUESTION_OPTIONS_MAX {
            return Err(invalid("selected", "maxItems"));
        }
        for (index, id) in self.selected.iter().enumerate() {
            check_id(&format!("selected.{index}"), id)?;
        }
        if let Some(text) = &self.text {
            check::max_len("text", text, QUESTION_ANSWER_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRecordDto {
    pub request: QuestionRequestDto,
    pub answers: Vec<QuestionAnswerDto>,
    pub skipped: bool,
}

impl Validate for QuestionRecordDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("request", self.request.validate())?;
        if self.skipped && !self.answers.is_empty() {
            return Err(invalid("answers", "skipped"));
        }
        validate_answers("answers", &self.answers)
    }
}

fn validate_answers(field: &str, answers: &[QuestionAnswerDto]) -> Result<(), AppError> {
    if answers.len() > QUESTIONS_MAX {
        return Err(invalid(field, "maxItems"));
    }
    for (index, answer) in answers.iter().enumerate() {
        nested(&format!("{field}.{index}"), answer.validate())?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAnswerInput {
    pub id: RunId,
    pub request_id: String,
    pub answers: Vec<QuestionAnswerDto>,
}

impl Validate for ChatAnswerInput {
    fn validate(&self) -> Result<(), AppError> {
        check_request_id("requestId", &self.request_id)?;
        if self.answers.is_empty() {
            return Err(invalid("answers", "minItems"));
        }
        validate_answers("answers", &self.answers)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSkipInput {
    pub id: RunId,
    pub request_id: String,
}

impl Validate for ChatSkipInput {
    fn validate(&self) -> Result<(), AppError> {
        check_request_id("requestId", &self.request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::{Value, json};

    const RUN: &str = "0199aa11-1111-7111-8111-111111111111";
    const REQUEST: &str = "0199aa11-2222-7222-8222-222222222222";

    fn choice(options: Value) -> Value {
        json!({
            "id": REQUEST,
            "runId": RUN,
            "questions": [{
                "id": "q1",
                "title": "Формат отчёта",
                "kind": "single",
                "options": options,
                "allowOther": false
            }]
        })
    }

    #[test]
    fn a_choice_needs_two_distinct_options_and_one_recommendation_at_most() {
        let one = json!({ "id": "o1", "label": "DOCX", "recommended": false });
        let two = json!({ "id": "o2", "label": "PDF", "recommended": true });
        assert!(parse::<QuestionRequestDto>(choice(json!([one, two]))).is_ok());
        assert!(parse::<QuestionRequestDto>(choice(json!([one]))).is_err());
        assert!(parse::<QuestionRequestDto>(choice(json!([one, one]))).is_err());
        let three = json!({ "id": "o3", "label": "HTML", "recommended": true });
        assert!(parse::<QuestionRequestDto>(choice(json!([one, two, three]))).is_err());
    }

    #[test]
    fn a_text_question_has_no_options_and_seven_questions_are_too_many() {
        let text = json!({
            "id": "q1",
            "title": "Название",
            "kind": "text",
            "options": [],
            "allowOther": false
        });
        let request =
            |questions: Vec<Value>| json!({ "id": REQUEST, "runId": RUN, "questions": questions });
        assert!(parse::<QuestionRequestDto>(request(vec![text.clone()])).is_ok());
        let many: Vec<Value> = (1..=7)
            .map(|index| {
                let mut question = text.clone();
                question["id"] = json!(format!("q{index}"));
                question
            })
            .collect();
        assert!(parse::<QuestionRequestDto>(request(many)).is_err());
        let mut other = text;
        other["allowOther"] = json!(true);
        assert!(parse::<QuestionRequestDto>(request(vec![other])).is_err());
    }
}
