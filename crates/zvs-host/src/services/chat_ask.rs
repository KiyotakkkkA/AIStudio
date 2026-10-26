use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Value, json};
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AppError, ConversationId, QUESTIONS_MAX, QuestionAnswerDto, QuestionDto, QuestionKind,
    QuestionOptionDto, QuestionRequestDto, RunId, Validate,
};

use super::chat_tools::{OfferedTool, wire_tool_name};
use crate::kernel::QuestionOutcome;
use crate::platform::ids::new_id;

pub const ASK_TOOL: &str = "chat.ask";
pub const ASK_TOOL_NAME: &str = "ask_user";
pub const ASK_DESCRIPTION: &str = "Задать пользователю от 1 до 6 вопросов и дождаться ответа. Вопрос — свободный текст (text), выбор одного варианта (single) или нескольких (multiple); у вопроса с выбором от 2 до 8 вариантов, не больше одного с recommended: true. allowOther разрешает свой вариант. Ответы возвращаются результатом вызова.";
pub const ASK_HEADING: &str = "Questions to the user";
pub const ASK_RULES: &str = "When the request is ambiguous and the answer changes what you do, call the ask tool instead of guessing. Ask few, short questions, offer concrete options where you can, and mark the option you would choose as recommended. The turn pauses until the user answers, and the answers come back as the call result. Do not ask what the other tools can tell you, and do not ask again what the user has already answered.";
pub const ASK_ONCE: &str = "В одном раунде допускается один вызов chat.ask";
pub const ASK_MALFORMED: &str = "Вопросы сформулированы некорректно";
pub const ASK_SKIPPED: &str = "The user closed the questions without answering. Continue with reasonable assumptions and state them in the answer.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskArguments {
    questions: Vec<AskedQuestion>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskedQuestion {
    title: String,
    #[serde(default)]
    kind: Option<QuestionKind>,
    #[serde(default)]
    options: Vec<AskedOption>,
    #[serde(default)]
    allow_other: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskedOption {
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    recommended: bool,
}

pub fn ask_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "questions": {
                "type": "array",
                "minItems": 1,
                "maxItems": QUESTIONS_MAX,
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Вопрос одной фразой" },
                        "kind": { "type": "string", "enum": ["text", "single", "multiple"] },
                        "options": {
                            "type": "array",
                            "maxItems": 8,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string" },
                                    "description": { "type": "string" },
                                    "recommended": { "type": "boolean" }
                                },
                                "required": ["label"]
                            }
                        },
                        "allowOther": { "type": "boolean" }
                    },
                    "required": ["title", "kind"]
                }
            }
        },
        "required": ["questions"]
    })
}

pub fn ask_tool(offered: &[OfferedTool]) -> OfferedTool {
    let taken: HashSet<String> = offered.iter().map(|tool| tool.name.clone()).collect();
    let name = if taken.contains(ASK_TOOL_NAME) {
        wire_tool_name(ASK_TOOL, &taken)
    } else {
        ASK_TOOL_NAME.to_owned()
    };
    OfferedTool {
        key: ASK_TOOL.to_owned(),
        name,
        description: ASK_DESCRIPTION.to_owned(),
        parameters: ask_parameters(),
        destructive: false,
        read_only: false,
        source: zvs_contract::OfferedToolSource::Base,
        server: None,
    }
}

pub fn ask_section(offered: &[OfferedTool]) -> String {
    offered
        .iter()
        .find(|tool| tool.key == ASK_TOOL)
        .map_or_else(String::new, |tool| {
            format!("{ASK_HEADING}\n{ASK_RULES} The ask tool is {}.", tool.name)
        })
}

fn problem(error: &AppError) -> String {
    let detail = |key: &str| {
        error
            .details
            .as_ref()
            .and_then(|details| details.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match (detail("field"), detail("rule"), detail("reason")) {
        (Some(field), Some(rule), _) => {
            format!(
                "Invalid questions: {field} breaks the {rule} rule. Fix the call and ask again."
            )
        }
        (_, _, Some(reason)) => format!("Invalid questions: {reason}. Fix the call and ask again."),
        _ => "Invalid questions. Fix the call and ask again.".to_owned(),
    }
}

pub fn ask_request(
    arguments: &Value,
    run_id: RunId,
    conversation_id: Option<ConversationId>,
) -> Result<QuestionRequestDto, String> {
    let parsed: AskArguments = serde_json::from_value(arguments.clone())
        .map_err(|error| format!("Invalid questions: {error}. Fix the call and ask again."))?;
    let questions = parsed
        .questions
        .into_iter()
        .enumerate()
        .map(|(index, question)| {
            let kind = question.kind.unwrap_or(if question.options.is_empty() {
                QuestionKind::Text
            } else {
                QuestionKind::Single
            });
            QuestionDto {
                id: format!("q{}", index + 1),
                title: js_trim(&question.title).to_owned(),
                kind,
                options: question
                    .options
                    .into_iter()
                    .enumerate()
                    .map(|(position, option)| QuestionOptionDto {
                        id: format!("o{}", position + 1),
                        label: js_trim(&option.label).to_owned(),
                        description: option
                            .description
                            .as_deref()
                            .map(js_trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_owned),
                        recommended: option.recommended,
                    })
                    .collect(),
                allow_other: question.allow_other && kind != QuestionKind::Text,
            }
        })
        .collect();
    let request = QuestionRequestDto {
        id: new_id().hyphenated().to_string(),
        run_id,
        conversation_id,
        questions,
    };
    request.validate().map_err(|error| problem(&error))?;
    Ok(request)
}

fn answer_line(question: &QuestionDto, answer: Option<&QuestionAnswerDto>) -> String {
    let Some(answer) = answer else {
        return "(no answer)".to_owned();
    };
    let mut parts: Vec<String> = answer
        .selected
        .iter()
        .filter_map(|id| question.options.iter().find(|option| &option.id == id))
        .map(|option| option.label.clone())
        .collect();
    if let Some(text) = &answer.text {
        parts.push(if question.kind == QuestionKind::Text {
            text.clone()
        } else {
            format!("Other: {text}")
        });
    }
    parts.join("; ")
}

pub fn answers_text(request: &QuestionRequestDto, outcome: &QuestionOutcome) -> String {
    let QuestionOutcome::Answered(answers) = outcome else {
        return ASK_SKIPPED.to_owned();
    };
    let lines: Vec<String> = request
        .questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let answer = answers
                .iter()
                .find(|answer| answer.question_id == question.id);
            format!(
                "{}. {}\n   {}",
                index + 1,
                question.title,
                answer_line(question, answer)
            )
        })
        .collect();
    format!("The user answered:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "0199aa11-1111-7111-8111-111111111111";

    fn run() -> RunId {
        RunId::parse(RUN).expect("a run id")
    }

    #[test]
    fn the_host_assigns_ids_and_infers_a_missing_kind() {
        let request = ask_request(
            &json!({ "questions": [
                { "title": " Формат? ", "options": [
                    { "label": "DOCX", "recommended": true },
                    { "label": "PDF", "description": " " }
                ] },
                { "title": "Название" }
            ] }),
            run(),
            None,
        )
        .expect("a request");
        assert_eq!(request.questions[0].id, "q1");
        assert_eq!(request.questions[0].title, "Формат?");
        assert_eq!(request.questions[0].kind, QuestionKind::Single);
        assert_eq!(request.questions[0].options[1].id, "o2");
        assert_eq!(request.questions[0].options[1].description, None);
        assert_eq!(request.questions[1].kind, QuestionKind::Text);
    }

    #[test]
    fn a_malformed_call_explains_itself_to_the_model() {
        let problem = ask_request(
            &json!({ "questions": [{ "title": "Формат?", "kind": "single", "options": [
                { "label": "DOCX" }
            ] }] }),
            run(),
            None,
        )
        .expect_err("one option is not a choice");
        assert!(problem.contains("questions.0.options"), "{problem}");
        assert!(ask_request(&json!({ "questions": "x" }), run(), None).is_err());
    }

    #[test]
    fn answers_read_back_as_labels_other_text_and_gaps() {
        let request = ask_request(
            &json!({ "questions": [
                { "title": "Формат?", "kind": "single", "allowOther": true, "options": [
                    { "label": "DOCX" }, { "label": "PDF" }
                ] },
                { "title": "Разделы?", "kind": "multiple", "options": [
                    { "label": "Введение" }, { "label": "Выводы" }
                ] },
                { "title": "Название?", "kind": "text" }
            ] }),
            run(),
            None,
        )
        .expect("a request");
        let text = answers_text(
            &request,
            &QuestionOutcome::Answered(vec![
                QuestionAnswerDto {
                    question_id: "q1".to_owned(),
                    selected: Vec::new(),
                    text: Some("HTML".to_owned()),
                },
                QuestionAnswerDto {
                    question_id: "q2".to_owned(),
                    selected: vec!["o1".to_owned(), "o2".to_owned()],
                    text: None,
                },
            ]),
        );
        assert_eq!(
            text,
            "The user answered:\n1. Формат?\n   Other: HTML\n2. Разделы?\n   Введение; Выводы\n3. Название?\n   (no answer)"
        );
        assert_eq!(
            answers_text(&request, &QuestionOutcome::Skipped),
            ASK_SKIPPED
        );
    }
}
