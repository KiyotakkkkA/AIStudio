use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Number, Value};

#[derive(Deserialize)]
struct Capabilities {
    think: Option<bool>,
    reasoning_effort: Option<bool>,
}

#[derive(Deserialize)]
struct MetaInfo {
    thinking_effort_level: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ModelMeta {
    capabilities: Option<Capabilities>,
    info: Option<MetaInfo>,
    hidden: Option<bool>,
}

#[derive(Deserialize)]
struct ModelParams {
    max_tokens: Option<Number>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: Option<String>,
    is_active: Option<bool>,
    meta: Option<ModelMeta>,
    params: Option<ModelParams>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    name: Option<String>,
    info: Option<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelsPayload {
    data: Option<Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaiModel {
    pub id: String,
    pub display_name: String,
    pub max_output: Option<i64>,
    pub thinking: bool,
    pub reasoning_efforts: Vec<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelList {
    pub models: Vec<ZaiModel>,
    pub skipped: usize,
}

const DEFAULT_EFFORTS: [&str; 2] = ["high", "max"];
const KNOWN_EFFORTS: [&str; 3] = ["low", "high", "max"];

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_owned)
}

fn positive(number: Option<&Number>) -> Option<i64> {
    number.and_then(Number::as_i64).filter(|value| *value > 0)
}

fn efforts(meta: Option<&ModelMeta>) -> Vec<String> {
    let capable = meta
        .and_then(|meta| meta.capabilities.as_ref())
        .and_then(|capabilities| capabilities.reasoning_effort)
        .unwrap_or(false);
    if !capable {
        return Vec::new();
    }
    let listed = meta
        .and_then(|meta| meta.info.as_ref())
        .and_then(|info| info.thinking_effort_level.as_deref())
        .unwrap_or_default();
    let known: Vec<String> = KNOWN_EFFORTS
        .iter()
        .filter(|effort| listed.iter().any(|level| level == *effort))
        .map(|effort| (*effort).to_owned())
        .collect();
    if known.is_empty() {
        DEFAULT_EFFORTS
            .iter()
            .map(|effort| (*effort).to_owned())
            .collect()
    } else {
        known
    }
}

fn to_model(id: String, entry: &ModelEntry) -> ZaiModel {
    let info = entry.info.as_ref();
    let meta = info.and_then(|info| info.meta.as_ref());
    ZaiModel {
        display_name: text(entry.name.as_deref())
            .or_else(|| text(info.and_then(|info| info.name.as_deref())))
            .unwrap_or_else(|| id.clone()),
        max_output: positive(
            info.and_then(|info| info.params.as_ref())
                .and_then(|params| params.max_tokens.as_ref()),
        ),
        thinking: meta
            .and_then(|meta| meta.capabilities.as_ref())
            .and_then(|capabilities| capabilities.think)
            .unwrap_or(false),
        reasoning_efforts: efforts(meta),
        hidden: meta.and_then(|meta| meta.hidden).unwrap_or(false),
        id,
    }
}

pub fn parse_models(payload: &Value) -> Option<ModelList> {
    let entries = ModelsPayload::deserialize(payload).ok()?.data?;
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut skipped = 0;
    for value in entries {
        let Ok(entry) = ModelEntry::deserialize(&value) else {
            skipped += 1;
            continue;
        };
        let id = entry.id.trim();
        if id.is_empty() || seen.contains(id) {
            continue;
        }
        if entry.info.as_ref().and_then(|info| info.is_active) == Some(false) {
            continue;
        }
        let model = to_model(id.to_owned(), &entry);
        seen.insert(model.id.clone());
        models.push(model);
    }
    Some(ModelList { models, skipped })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rows_keep_their_thinking_efforts_and_output_limit() {
        let payload = json!({ "data": [
            { "id": "glm-5.3", "name": "GLM-5.3", "info": {
                "is_active": true,
                "params": { "max_tokens": 128_000, "temperature": 1 },
                "meta": {
                    "capabilities": { "think": true, "reasoning_effort": true },
                    "info": { "thinking_effort_level": ["max", "low", "turbo"] },
                },
            } },
            { "id": "glm-5.2", "name": " ", "info": {
                "name": "GLM-5.2",
                "meta": { "capabilities": { "think": true, "reasoning_effort": true } },
            } },
            { "id": "glm-4-air-250414", "name": "GLM-4-32B", "info": {
                "meta": { "hidden": true, "capabilities": { "think": false } },
            } },
        ] });
        let list = parse_models(&payload).expect("a model list");
        assert_eq!(list.skipped, 0);
        let [fresh, plain, hidden] = list.models.as_slice() else {
            panic!("three models");
        };
        assert_eq!(fresh.max_output, Some(128_000));
        assert_eq!(fresh.reasoning_efforts, ["low", "max"]);
        assert_eq!(plain.display_name, "GLM-5.2");
        assert_eq!(plain.reasoning_efforts, ["high", "max"]);
        assert!(hidden.hidden && !hidden.thinking);
        assert!(hidden.reasoning_efforts.is_empty());
    }

    #[test]
    fn inactive_duplicate_and_unknown_rows_are_left_out() {
        let payload = json!({ "data": [
            { "id": "a", "info": { "is_active": false } },
            { "id": "b" },
            { "id": "b", "name": "again" },
            { "id": 7 },
            { "id": "" },
        ] });
        let list = parse_models(&payload).expect("a model list");
        assert_eq!(list.skipped, 1);
        assert_eq!(list.models.len(), 1);
        assert_eq!(list.models[0].display_name, "b");
        assert_eq!(parse_models(&json!({ "detail": "x" })), None);
        assert_eq!(parse_models(&json!({ "data": {} })), None);
    }
}
