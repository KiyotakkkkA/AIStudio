use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Number, Value};

const DIAGNOSTIC_FIELDS: [&str; 16] = [
    "id",
    "name",
    "object",
    "owned_by",
    "info",
    "is_active",
    "meta",
    "max_context_length",
    "max_generation_length",
    "capabilities",
    "abilities",
    "modality",
    "data",
    "success",
    "error",
    "code",
];
const ROOT_FIELDS: [&str; 6] = ["success", "data", "error", "code", "detail", "message"];

/// A capability flag: Qwen has sent booleans, numbers and, occasionally, strings.
enum Flag {
    Bool(bool),
    Number(f64),
    Other,
}

impl<'de> Deserialize<'de> for Flag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Bool(value) => Ok(Self::Bool(value)),
            Value::Number(value) => Ok(value.as_f64().map_or(Self::Other, Self::Number)),
            _ => Ok(Self::Other),
        }
    }
}

#[derive(Deserialize)]
struct Flags {
    thinking: Option<Flag>,
}

#[derive(Deserialize)]
struct ModelMeta {
    max_context_length: Option<Number>,
    max_generation_length: Option<Number>,
    capabilities: Option<Flags>,
    abilities: Option<Flags>,
    auto_thinking: Option<Flag>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: Option<String>,
    is_active: Option<bool>,
    meta: Option<ModelMeta>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    name: Option<String>,
    owned_by: Option<String>,
    info: Option<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelsPayload {
    success: Option<bool>,
    data: Option<Value>,
}

/// One model the account can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QwenModel {
    pub id: String,
    pub display_name: String,
    pub owned_by: Option<String>,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    /// Whether turns on this model run in `Thinking` mode rather than `Fast`.
    pub thinking: bool,
    pub auto_thinking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelList {
    pub models: Vec<QwenModel>,
    /// Rows dropped because their shape is unknown; valid rows are kept.
    pub skipped: usize,
    /// Why rows were dropped, for the first [`MAX_ROW_PROBLEMS`] of them.
    pub problems: Vec<RowProblem>,
}

pub const MAX_ROW_PROBLEMS: usize = 10;

/// Where a dropped row departs from the known schema: a field path and JSON kinds, never the
/// value itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowProblem {
    pub index: usize,
    pub field: &'static str,
    pub expected: &'static str,
    pub found: &'static str,
}

fn non_null<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|value| !value.is_null())
}

/// Find the first field of `row` that the schema rejects.
fn row_problem(index: usize, row: &Value) -> RowProblem {
    let problem = |field, expected, found: &Value| RowProblem {
        index,
        field,
        expected,
        found: shape(found),
    };
    if !row.is_object() {
        return problem("<row>", "object", row);
    }
    match row.get("id") {
        Some(Value::String(_)) => {}
        Some(other) => return problem("id", "string", other),
        None => {
            return RowProblem {
                index,
                field: "id",
                expected: "string",
                found: "missing",
            };
        }
    }
    for field in ["name", "owned_by"] {
        if let Some(value) = non_null(row, field).filter(|value| !value.is_string()) {
            return problem(field, "string", value);
        }
    }
    let Some(info) = non_null(row, "info") else {
        return problem("<row>", "known schema", row);
    };
    if !info.is_object() {
        return problem("info", "object", info);
    }
    if let Some(value) = non_null(info, "name").filter(|value| !value.is_string()) {
        return problem("info.name", "string", value);
    }
    if let Some(value) = non_null(info, "is_active").filter(|value| !value.is_boolean()) {
        return problem("info.is_active", "boolean", value);
    }
    let Some(meta) = non_null(info, "meta") else {
        return problem("<row>", "known schema", row);
    };
    if !meta.is_object() {
        return problem("info.meta", "object", meta);
    }
    for (key, field) in [
        ("max_context_length", "info.meta.max_context_length"),
        ("max_generation_length", "info.meta.max_generation_length"),
    ] {
        if let Some(value) = non_null(meta, key) {
            let whole_positive = value
                .as_number()
                .is_some_and(|number| matches!(positive(Some(number)), Ok(Some(_))));
            if !whole_positive {
                return problem(field, "positive integer", value);
            }
        }
    }
    for (key, field) in [
        ("capabilities", "info.meta.capabilities"),
        ("abilities", "info.meta.abilities"),
    ] {
        if let Some(value) = non_null(meta, key).filter(|value| !value.is_object()) {
            return problem(field, "object", value);
        }
    }
    problem("<row>", "known schema", row)
}

fn positive(value: Option<&Number>) -> Result<Option<i64>, ()> {
    let Some(number) = value else {
        return Ok(None);
    };
    let whole = number
        .as_i64()
        .or_else(|| {
            number
                .as_f64()
                .filter(|float| float.fract() == 0.0)
                .map(|float| float as i64)
        })
        .ok_or(())?;
    if whole > 0 { Ok(Some(whole)) } else { Err(()) }
}

fn enabled(flag: Option<&Flag>) -> bool {
    match flag {
        Some(Flag::Bool(value)) => *value,
        Some(Flag::Number(value)) => *value > 0.0,
        Some(Flag::Other) | None => false,
    }
}

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_owned)
}

fn to_model(id: String, entry: &ModelEntry) -> Result<QwenModel, ()> {
    let meta = entry.info.as_ref().and_then(|info| info.meta.as_ref());
    let context_window = positive(meta.and_then(|meta| meta.max_context_length.as_ref()))?;
    let max_output = positive(meta.and_then(|meta| meta.max_generation_length.as_ref()))?;
    // An explicit `capabilities.thinking` wins over the legacy `abilities.thinking`.
    let thinking = meta
        .and_then(|meta| meta.capabilities.as_ref())
        .and_then(|flags| flags.thinking.as_ref())
        .or_else(|| {
            meta.and_then(|meta| meta.abilities.as_ref())
                .and_then(|flags| flags.thinking.as_ref())
        });
    let thinking = enabled(thinking);
    Ok(QwenModel {
        auto_thinking: thinking && enabled(meta.and_then(|meta| meta.auto_thinking.as_ref())),
        display_name: text(entry.name.as_deref())
            .or_else(|| text(entry.info.as_ref().and_then(|info| info.name.as_deref())))
            .unwrap_or_else(|| id.clone()),
        owned_by: text(entry.owned_by.as_deref()),
        context_window,
        max_output,
        thinking,
        id,
    })
}

/// Decode `GET /api/v2/models/`. `None` means the envelope itself is unusable: an explicit
/// `success: false` or a `data` that is neither a list nor `{ "data": [...] }`.
pub fn parse_models(payload: &Value) -> Option<ModelList> {
    let parsed = ModelsPayload::deserialize(payload).ok()?;
    if parsed.success == Some(false) {
        return None;
    }
    let entries = match parsed.data? {
        Value::Array(entries) => entries,
        Value::Object(mut data) => match data.remove("data")? {
            Value::Array(entries) => entries,
            _ => return None,
        },
        _ => return None,
    };
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut skipped = 0;
    let mut problems = Vec::new();
    let mut skip = |index: usize, row: &Value| {
        skipped += 1;
        if problems.len() < MAX_ROW_PROBLEMS {
            problems.push(row_problem(index, row));
        }
    };
    for (index, value) in entries.into_iter().enumerate() {
        let Ok(entry) = ModelEntry::deserialize(&value) else {
            skip(index, &value);
            continue;
        };
        let id = entry.id.trim();
        if id.is_empty() || seen.contains(id) {
            continue;
        }
        if entry.info.as_ref().and_then(|info| info.is_active) == Some(false) {
            continue;
        }
        let Ok(model) = to_model(id.to_owned(), &entry) else {
            skip(index, &value);
            continue;
        };
        seen.insert(model.id.clone());
        models.push(model);
    }
    Some(ModelList {
        models,
        skipped,
        problems,
    })
}

/// The shape of a model-list response that could not be used: field names, kinds and counts,
/// never values, so it can be logged without leaking the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelsDiagnostics {
    pub payload_shape: &'static str,
    pub root_fields: Vec<&'static str>,
    pub outer_success: String,
    pub data_shape: &'static str,
    pub data_fields: Vec<&'static str>,
    pub model_count: usize,
    pub sample_model_fields: Vec<&'static str>,
    pub sample_info_fields: Vec<&'static str>,
    pub sample_meta_fields: Vec<&'static str>,
    /// Rows the schema rejected, when the envelope itself is usable.
    pub row_problems: Vec<RowProblem>,
    /// Every field of `data` with its JSON kind (`name:kind`), up to 20.
    pub data_field_kinds: Vec<String>,
    /// The vendor's error code from `data.code` or `code`, when it looks like a code.
    pub vendor_code: Option<String>,
}

const MAX_KIND_FIELDS: usize = 20;
const MAX_CODE_CHARS: usize = 64;

/// A vendor error code is a short identifier or number, unlike free text that might carry data.
fn error_code(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text)
            if !text.is_empty()
                && text.len() <= MAX_CODE_CHARS
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) =>
        {
            Some(text.clone())
        }
        Value::String(_) => Some("<free text>".to_owned()),
        _ => None,
    }
}

const fn shape(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
    }
}

fn present(value: Option<&Value>, fields: &[&'static str]) -> Vec<&'static str> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            fields
                .iter()
                .copied()
                .filter(|field| object.contains_key(*field))
                .collect()
        })
        .unwrap_or_default()
}

impl ModelsDiagnostics {
    pub fn inspect(body: &str) -> Self {
        let parsed = serde_json::from_str::<Value>(body).ok();
        let root = parsed.as_ref().filter(|value| value.is_object());
        let data = root.and_then(|root| root.get("data"));
        let models = data.and_then(Value::as_array).or_else(|| {
            data.and_then(|data| data.get("data"))
                .and_then(Value::as_array)
        });
        let sample = models.and_then(|models| models.first());
        let info = sample.and_then(|model| model.get("info"));
        Self {
            payload_shape: parsed.as_ref().map_or("invalid-json", shape),
            root_fields: present(root, &ROOT_FIELDS),
            outer_success: match root.and_then(|root| root.get("success")) {
                Some(Value::Bool(success)) => success.to_string(),
                Some(Value::Null) => "null".to_owned(),
                Some(_) => "invalid".to_owned(),
                None => "missing".to_owned(),
            },
            data_shape: data.map_or("missing", shape),
            data_fields: present(data, &DIAGNOSTIC_FIELDS),
            model_count: models.map_or(0, Vec::len),
            sample_model_fields: present(sample, &DIAGNOSTIC_FIELDS),
            sample_info_fields: present(info, &DIAGNOSTIC_FIELDS),
            sample_meta_fields: present(info.and_then(|info| info.get("meta")), &DIAGNOSTIC_FIELDS),
            row_problems: parsed
                .as_ref()
                .and_then(parse_models)
                .map(|list| list.problems)
                .unwrap_or_default(),
            data_field_kinds: data
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .take(MAX_KIND_FIELDS)
                        .map(|(name, value)| format!("{name}:{}", shape(value)))
                        .collect()
                })
                .unwrap_or_default(),
            vendor_code: data
                .and_then(|data| data.get("code"))
                .or_else(|| root.and_then(|root| root.get("code")))
                .and_then(error_code),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rows_are_decoded_deduplicated_and_filtered() {
        let list = parse_models(&json!({ "success": true, "data": { "data": [
            { "id": " qwen3.7-plus ", "name": "Qwen3.7-Plus", "owned_by": "qwen", "info": {
                "meta": { "max_context_length": 131_072, "max_generation_length": 32_768.0,
                          "capabilities": { "thinking": true } } } },
            { "id": "qwen3.7-plus", "name": "duplicate" },
            { "id": "retired", "info": { "is_active": false } },
            { "id": "legacy", "info": { "name": "Legacy", "meta": { "abilities": { "thinking": 1 } } } },
            { "id": "overridden", "info": { "meta": {
                "capabilities": { "thinking": 0 }, "abilities": { "thinking": true } } } },
        ] } }))
        .expect("decodes");
        assert_eq!(list.skipped, 0);
        let summary = list
            .models
            .iter()
            .map(|model| {
                (
                    model.id.as_str(),
                    model.display_name.as_str(),
                    model.thinking,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                ("qwen3.7-plus", "Qwen3.7-Plus", true),
                ("legacy", "Legacy", true),
                ("overridden", "overridden", false),
            ]
        );
        assert_eq!(list.models[0].context_window, Some(131_072));
        assert_eq!(list.models[0].max_output, Some(32_768));
        assert_eq!(list.models[0].owned_by.as_deref(), Some("qwen"));
    }

    #[test]
    fn auto_thinking_is_offered_only_where_the_meta_names_it_on_a_thinking_model() {
        let list = parse_models(&json!({ "data": [
            { "id": "qwen3.7-plus", "info": { "meta": {
                "capabilities": { "thinking": true }, "abilities": { "thinking": 3 },
                "auto_thinking": true, "thinking_format": "summary" } } },
            { "id": "qwen3.7-max", "info": { "meta": {
                "capabilities": { "document": true, "thinking": true },
                "abilities": { "document": 1, "mcp": 1, "thinking": 1, "parse_url": 2 },
                "auto_search": true, "thinking_format": "summary",
                "max_thinking_generation_length": 81_920 } } },
            { "id": "qwen3.8-omni-flash", "info": { "meta": {
                "capabilities": { "vision": true, "audio": true, "thinking": true },
                "abilities": { "vision": 1, "thinking": 1 },
                "chat_type": ["t2t", "search"] } } },
            { "id": "no-thinking", "info": { "meta": {
                "capabilities": { "thinking": false }, "auto_thinking": true } } },
        ] }))
        .expect("decodes");
        let modes = list
            .models
            .iter()
            .map(|model| (model.id.as_str(), model.thinking, model.auto_thinking))
            .collect::<Vec<_>>();
        assert_eq!(
            modes,
            [
                ("qwen3.7-plus", true, true),
                ("qwen3.7-max", true, false),
                ("qwen3.8-omni-flash", true, false),
                ("no-thinking", false, false),
            ]
        );
    }

    #[test]
    fn malformed_rows_are_counted_and_unusable_envelopes_refused() {
        let list = parse_models(&json!({ "data": [
            { "id": 42 },
            { "id": "zero", "info": { "meta": { "max_context_length": 0 } } },
            { "id": "odd", "info": { "meta": { "capabilities": { "thinking": "yes" } } } },
        ] }))
        .expect("decodes");
        assert_eq!(list.skipped, 2);
        assert_eq!(list.models.len(), 1);
        assert!(!list.models[0].thinking);
        assert_eq!(
            list.problems,
            [
                RowProblem {
                    index: 0,
                    field: "id",
                    expected: "string",
                    found: "number"
                },
                RowProblem {
                    index: 1,
                    field: "info.meta.max_context_length",
                    expected: "positive integer",
                    found: "number"
                },
            ]
        );
        for envelope in [
            json!({ "success": false, "data": [] }),
            json!({ "success": true, "data": "x" }),
            json!({ "success": true, "data": { "data": "x" } }),
            json!({ "success": true }),
            json!([]),
        ] {
            assert_eq!(parse_models(&envelope), None, "{envelope}");
        }
    }

    #[test]
    fn diagnostics_name_fields_without_values() {
        let secret = "SUPERSECRET";
        let body = json!({ "success": true, "data": { "data": [
            { "id": secret, "info": { "meta": { "modality": [secret] } } }
        ], "token": secret } })
        .to_string();
        let diagnostics = ModelsDiagnostics::inspect(&body);
        assert_eq!(diagnostics.payload_shape, "object");
        assert_eq!(diagnostics.root_fields, ["success", "data"]);
        assert_eq!(diagnostics.outer_success, "true");
        assert_eq!(diagnostics.data_shape, "object");
        assert_eq!(diagnostics.data_fields, ["data"]);
        assert_eq!(diagnostics.model_count, 1);
        assert_eq!(diagnostics.sample_model_fields, ["id", "info"]);
        assert_eq!(diagnostics.sample_info_fields, ["meta"]);
        assert_eq!(diagnostics.sample_meta_fields, ["modality"]);
        assert!(!format!("{diagnostics:?}").contains(secret));
        let refused = ModelsDiagnostics::inspect(
            &json!({ "success": false, "data": { "code": "Not_Login", "details": "x" } })
                .to_string(),
        );
        assert_eq!(refused.vendor_code.as_deref(), Some("Not_Login"));
        assert_eq!(refused.data_field_kinds, ["code:string", "details:string"]);
        let free = ModelsDiagnostics::inspect(
            &json!({ "success": false, "data": { "code": "token SUPERSECRET leaked" } })
                .to_string(),
        );
        assert_eq!(free.vendor_code.as_deref(), Some("<free text>"));
        let html = ModelsDiagnostics::inspect("<!doctype html>");
        assert_eq!(
            (html.payload_shape, html.data_shape),
            ("invalid-json", "missing")
        );
        assert_eq!(html.outer_success, "missing");
    }
}
