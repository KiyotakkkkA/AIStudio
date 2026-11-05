use serde_json::{Map, Value};
use zvs_contract::{AppError, AppErrorCode, VALIDATION_MESSAGE};

pub const INVALID_ARGUMENTS: &str = "Некорректные аргументы вызова";
pub const ARGUMENTS_LEAD: &str = "Аргументы вызова не прошли проверку.";
pub const ARGUMENTS_ROOT: &str = "аргументы";
pub const MISSING_FIELD: &str = "обязательное поле отсутствует";
pub const JSON_IN_STRING: &str = "JSON был ошибочно передан в виде строки";
pub const NO_VARIANT: &str = "не подходит ни один из вариантов схемы";
pub const SHAPE_LEAD: &str = "Ожидаемая форма аргументов:";
pub const RETRY_HINT: &str = "Исправьте аргументы и повторите вызов.";
pub const RULE_LEAD: &str = "нарушено правило";
pub const REASON_LEAD: &str = "Причина:";

const PROBLEMS_MAX: usize = 8;
const DEPTH_MAX: usize = 24;
const SHAPE_DEPTH_MAX: usize = 6;
const SHAPE_CHARS_MAX: usize = 800;
const VALUES_SHOWN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub text: String,
}

pub fn arguments_failure(error: &AppError, schema: &Value, arguments: &Value) -> Option<String> {
    if error.code != AppErrorCode::ValidationFailed || error.message != VALIDATION_MESSAGE {
        return None;
    }
    let mut lines = vec![ARGUMENTS_LEAD.to_owned()];
    let problems = check(schema, arguments);
    if problems.is_empty() {
        let details = error.details.as_ref();
        let field = details
            .and_then(|details| details.get("field"))
            .and_then(Value::as_str);
        let rule = details
            .and_then(|details| details.get("rule"))
            .and_then(Value::as_str);
        let reason = details
            .and_then(|details| details.get("reason"))
            .and_then(Value::as_str);
        match (field, rule) {
            (Some(field), Some(rule)) => lines.push(format!("- {field}: {RULE_LEAD} {rule}")),
            (Some(field), None) => lines.push(format!("- {field}")),
            _ => {}
        }
        if let Some(reason) = reason {
            lines.push(format!("{REASON_LEAD} {reason}"));
        }
    } else {
        lines.extend(
            problems
                .iter()
                .map(|problem| format!("- {}: {}", problem.path, problem.text)),
        );
    }
    if let Some(shape) = shape_text(schema) {
        lines.push(format!("{SHAPE_LEAD} {shape}"));
    }
    lines.push(RETRY_HINT.to_owned());
    Some(lines.join("\n"))
}

pub fn check(schema: &Value, value: &Value) -> Vec<Problem> {
    let mut problems = Vec::new();
    visit(schema, schema, value, "", 0, &mut problems);
    problems.truncate(PROBLEMS_MAX);
    problems
}

fn shown_path(path: &str) -> String {
    if path.is_empty() {
        ARGUMENTS_ROOT.to_owned()
    } else {
        path.to_owned()
    }
}

fn field_path(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}.{name}")
    }
}

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let mut current = schema;
    for _ in 0..DEPTH_MAX {
        let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
            return current;
        };
        let Some(pointer) = reference.strip_prefix('#') else {
            return current;
        };
        match root.pointer(pointer) {
            Some(target) => current = target,
            None => return current,
        }
    }
    current
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub fn type_label(name: &str) -> &str {
    match name {
        "null" => "null",
        "boolean" => "логическое значение",
        "integer" => "целое число",
        "number" => "число",
        "string" => "строка",
        "array" => "массив",
        "object" => "объект",
        other => other,
    }
}

fn matches_type(expected: &str, value: &Value) -> bool {
    match expected {
        "integer" => match value {
            Value::Number(number) => {
                number.is_i64()
                    || number.is_u64()
                    || number.as_f64().is_some_and(|float| float.fract() == 0.0)
            }
            _ => false,
        },
        "number" => value.is_number(),
        other => type_name(value) == other,
    }
}

fn expected_types(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn type_problem(expected: &[&str], value: &Value) -> String {
    let wanted = expected
        .iter()
        .map(|name| type_label(name))
        .collect::<Vec<_>>()
        .join(" или ");
    let mut text = format!(
        "ожидается {wanted}, получено: {}",
        type_label(type_name(value))
    );
    if let Value::String(raw) = value
        && let Ok(inner) = serde_json::from_str::<Value>(raw.trim())
        && expected.iter().any(|name| matches_type(name, &inner))
    {
        text.push_str("; ");
        text.push_str(JSON_IN_STRING);
    }
    text
}

fn listed(values: &[Value]) -> String {
    let mut shown: Vec<String> = values
        .iter()
        .take(VALUES_SHOWN)
        .map(|value| value.to_string())
        .collect();
    if values.len() > VALUES_SHOWN {
        shown.push("…".to_owned());
    }
    shown.join(", ")
}

fn visit(
    root: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    depth: usize,
    problems: &mut Vec<Problem>,
) {
    if depth > DEPTH_MAX || problems.len() >= PROBLEMS_MAX {
        return;
    }
    let schema = resolve(root, schema);
    let Value::Object(rules) = schema else {
        return;
    };
    let push = |problems: &mut Vec<Problem>, text: String| {
        problems.push(Problem {
            path: shown_path(path),
            text,
        });
    };
    let expected = expected_types(schema);
    if !expected.is_empty() && !expected.iter().any(|name| matches_type(name, value)) {
        push(problems, type_problem(&expected, value));
        return;
    }
    if let Some(Value::Array(allowed)) = rules.get("enum")
        && !allowed.contains(value)
    {
        push(
            problems,
            format!("допустимые значения: {}", listed(allowed)),
        );
        return;
    }
    if let Some(constant) = rules.get("const")
        && constant != value
    {
        push(problems, format!("ожидается значение {constant}"));
        return;
    }
    if let Some(Value::Array(branches)) = rules.get("anyOf").or_else(|| rules.get("oneOf")) {
        variants(root, branches, value, path, depth, problems);
    }
    if let Some(Value::Array(parts)) = rules.get("allOf") {
        for part in parts {
            visit(root, part, value, path, depth + 1, problems);
        }
    }
    match value {
        Value::Object(fields) => object(root, rules, fields, path, depth, problems),
        Value::Array(items) => array(root, rules, items, path, depth, problems),
        _ => {}
    }
}

fn variants(
    root: &Value,
    branches: &[Value],
    value: &Value,
    path: &str,
    depth: usize,
    problems: &mut Vec<Problem>,
) {
    if let Some((name, allowed)) = discriminator(root, branches, value) {
        problems.push(Problem {
            path: field_path(path, &name),
            text: format!("допустимые значения: {}", listed(&allowed)),
        });
        return;
    }
    let mut best: Option<Vec<Problem>> = None;
    for branch in branches {
        let mut found = Vec::new();
        visit(root, branch, value, path, depth + 1, &mut found);
        if found.is_empty() {
            return;
        }
        if best.as_ref().is_none_or(|best| found.len() < best.len()) {
            best = Some(found);
        }
    }
    match best {
        Some(found) if found.iter().any(|problem| problem.path != shown_path(path)) => {
            problems.extend(found);
        }
        Some(_) => problems.push(Problem {
            path: shown_path(path),
            text: NO_VARIANT.to_owned(),
        }),
        None => {}
    }
}

fn tag(root: &Value, branch: &Value, name: &str) -> Option<Value> {
    let property = resolve(root, branch).get("properties")?.get(name)?;
    let property = resolve(root, property);
    if let Some(constant) = property.get("const") {
        return Some(constant.clone());
    }
    match property.get("enum")?.as_array()?.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

fn discriminator(root: &Value, branches: &[Value], value: &Value) -> Option<(String, Vec<Value>)> {
    let fields = value.as_object()?;
    let first = resolve(root, branches.first()?)
        .get("properties")?
        .as_object()?;
    for name in first.keys() {
        let tags: Option<Vec<Value>> = branches
            .iter()
            .map(|branch| tag(root, branch, name))
            .collect();
        let Some(tags) = tags else {
            continue;
        };
        if fields.get(name).is_none_or(|given| !tags.contains(given)) {
            return Some((name.clone(), tags));
        }
        return None;
    }
    None
}

fn object(
    root: &Value,
    rules: &Map<String, Value>,
    fields: &Map<String, Value>,
    path: &str,
    depth: usize,
    problems: &mut Vec<Problem>,
) {
    if let Some(Value::Array(required)) = rules.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !fields.contains_key(name) {
                problems.push(Problem {
                    path: field_path(path, name),
                    text: MISSING_FIELD.to_owned(),
                });
            }
        }
    }
    if let Some(Value::Object(properties)) = rules.get("properties") {
        for (name, property) in properties {
            if let Some(field) = fields.get(name) {
                visit(
                    root,
                    property,
                    field,
                    &field_path(path, name),
                    depth + 1,
                    problems,
                );
            }
        }
    }
}

fn array(
    root: &Value,
    rules: &Map<String, Value>,
    items: &[Value],
    path: &str,
    depth: usize,
    problems: &mut Vec<Problem>,
) {
    let count = items.len();
    if let Some(minimum) = rules.get("minItems").and_then(Value::as_u64)
        && (count as u64) < minimum
    {
        problems.push(Problem {
            path: shown_path(path),
            text: format!("нужно не меньше {minimum} элементов, получено {count}"),
        });
    }
    if let Some(maximum) = rules.get("maxItems").and_then(Value::as_u64)
        && (count as u64) > maximum
    {
        problems.push(Problem {
            path: shown_path(path),
            text: format!("допускается не больше {maximum} элементов, получено {count}"),
        });
    }
    let prefix = rules
        .get("prefixItems")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    for (index, item) in items.iter().enumerate() {
        let schema = prefix.get(index).or_else(|| rules.get("items"));
        if let Some(schema) = schema {
            visit(
                root,
                schema,
                item,
                &format!("{}[{index}]", shown_path(path)),
                depth + 1,
                problems,
            );
        }
    }
}

pub fn repair(schema: &Value, value: Value) -> Value {
    mend(schema, schema, value, 0)
}

fn mend(root: &Value, schema: &Value, value: Value, depth: usize) -> Value {
    if depth > DEPTH_MAX {
        return value;
    }
    let schema = resolve(root, schema);
    let Value::Object(rules) = schema else {
        return value;
    };
    let mut value = match value {
        Value::String(raw) => decoded(root, schema, raw),
        other => other,
    };
    if let Some(Value::Array(parts)) = rules.get("allOf") {
        for part in parts {
            value = mend(root, part, value, depth + 1);
        }
    }
    if let Some(Value::Array(branches)) = rules.get("anyOf").or_else(|| rules.get("oneOf")) {
        return match branch(root, branches, &value) {
            Some(chosen) => mend(root, chosen, value, depth + 1),
            None => value,
        };
    }
    match value {
        Value::Object(mut fields) => {
            if let Some(Value::Object(properties)) = rules.get("properties") {
                for (name, property) in properties {
                    if let Some(field) = fields.remove(name) {
                        fields.insert(name.clone(), mend(root, property, field, depth + 1));
                    }
                }
            }
            Value::Object(fields)
        }
        Value::Array(items) => {
            let prefix = rules
                .get("prefixItems")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            Value::Array(
                items
                    .into_iter()
                    .enumerate()
                    .map(
                        |(index, item)| match prefix.get(index).or_else(|| rules.get("items")) {
                            Some(schema) => mend(root, schema, item, depth + 1),
                            None => item,
                        },
                    )
                    .collect(),
            )
        }
        other => other,
    }
}

fn accepted<'a>(root: &'a Value, schema: &'a Value) -> Option<Vec<&'a str>> {
    let mut names = expected_types(schema);
    let branches = schema
        .get("anyOf")
        .or_else(|| schema.get("oneOf"))
        .and_then(Value::as_array);
    if let Some(branches) = branches {
        for branch in branches {
            let types = expected_types(resolve(root, branch));
            if types.is_empty() {
                return None;
            }
            names.extend(types);
        }
    }
    (!names.is_empty()).then_some(names)
}

fn decoded(root: &Value, schema: &Value, raw: String) -> Value {
    let Some(types) = accepted(root, schema) else {
        return Value::String(raw);
    };
    if types.contains(&"string") || !types.iter().any(|name| matches!(*name, "array" | "object")) {
        return Value::String(raw);
    }
    let trimmed = raw.trim();
    if !trimmed.starts_with('[') && !trimmed.starts_with('{') {
        return Value::String(raw);
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(inner) if types.iter().any(|name| matches_type(name, &inner)) => inner,
        _ => Value::String(raw),
    }
}

fn branch<'a>(root: &'a Value, branches: &'a [Value], value: &Value) -> Option<&'a Value> {
    let fitting: Vec<&Value> = branches
        .iter()
        .filter(|branch| {
            let types = expected_types(resolve(root, branch));
            types.is_empty() || types.iter().any(|name| matches_type(name, value))
        })
        .collect();
    if let [only] = fitting.as_slice() {
        return Some(only);
    }
    let fields = value.as_object()?;
    fitting.into_iter().find(|branch| {
        resolve(root, branch)
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|properties| {
                properties.keys().any(|name| {
                    tag(root, branch, name).is_some_and(|tag| fields.get(name) == Some(&tag))
                })
            })
    })
}

pub fn shape(schema: &Value) -> Value {
    sketch(schema, schema, 0)
}

fn shape_text(schema: &Value) -> Option<String> {
    let value = shape(schema);
    if value.as_object().is_some_and(Map::is_empty) || value.is_null() {
        return None;
    }
    let text = value.to_string();
    if text.chars().count() > SHAPE_CHARS_MAX {
        return None;
    }
    Some(text)
}

fn sketch(root: &Value, schema: &Value, depth: usize) -> Value {
    let schema = resolve(root, schema);
    if depth > SHAPE_DEPTH_MAX {
        return Value::Null;
    }
    if let Some(example) = schema
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|examples| examples.first())
    {
        return example.clone();
    }
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    {
        return first.clone();
    }
    if let Some(first) = schema
        .get("anyOf")
        .or_else(|| schema.get("oneOf"))
        .and_then(Value::as_array)
        .and_then(|branches| branches.iter().find(|branch| !is_null(root, branch)))
    {
        return sketch(root, first, depth + 1);
    }
    let kind = expected_types(schema)
        .into_iter()
        .find(|name| *name != "null")
        .or_else(|| schema.get("properties").map(|_| "object"));
    match kind {
        Some("object") => {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let mut fields = Map::new();
            if let Some(Value::Object(properties)) = schema.get("properties") {
                for (name, property) in properties {
                    if required.contains(&name.as_str()) {
                        fields.insert(name.clone(), sketch(root, property, depth + 1));
                    }
                }
            }
            Value::Object(fields)
        }
        Some("array") => {
            if let Some(Value::Array(prefix)) = schema.get("prefixItems") {
                return Value::Array(
                    prefix
                        .iter()
                        .map(|item| sketch(root, item, depth + 1))
                        .collect(),
                );
            }
            schema.get("items").map_or_else(
                || Value::Array(Vec::new()),
                |items| Value::Array(vec![sketch(root, items, depth + 1)]),
            )
        }
        Some("string") => Value::String("…".to_owned()),
        Some("integer" | "number") => Value::from(0),
        Some("boolean") => Value::Bool(false),
        _ => Value::Null,
    }
}

fn is_null(root: &Value, schema: &Value) -> bool {
    expected_types(resolve(root, schema)) == ["null"]
}

#[cfg(test)]
mod tests;
