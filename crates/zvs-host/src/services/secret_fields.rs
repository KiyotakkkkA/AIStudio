use serde_json::{Value, json};
use zvs_contract::{FieldDescriptor, FieldKind, SecretFields, SecretTypeSchema};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIssue {
    pub path: String,
    pub code: &'static str,
}

impl FieldIssue {
    fn new(path: &str, code: &'static str) -> Self {
        Self {
            path: path.to_owned(),
            code,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "path": self.path, "code": self.code })
    }
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

fn check_value(field: &FieldDescriptor, value: &Value) -> Result<Value, &'static str> {
    match field.kind {
        FieldKind::Text => match value {
            Value::String(text) if field.required && text.is_empty() => Err("too_small"),
            Value::String(_) => Ok(value.clone()),
            _ => Err("invalid_type"),
        },
        FieldKind::Url => {
            let Value::String(text) = value else {
                return Err("invalid_type");
            };
            let trimmed = js_trim(text);
            url::Url::parse(trimmed).map_err(|_| "invalid_format")?;
            Ok(Value::String(
                trimmed
                    .chars()
                    .filter(|character| !matches!(character, '\t' | '\n' | '\r'))
                    .collect(),
            ))
        }
        FieldKind::Number => match value {
            Value::Number(_) => Ok(value.clone()),
            _ => Err("invalid_type"),
        },
        FieldKind::Boolean => match value {
            Value::Bool(_) => Ok(value.clone()),
            _ => Err("invalid_type"),
        },
        FieldKind::Select => {
            let allowed = field.options.iter().flatten().any(|option| {
                value
                    .as_str()
                    .is_some_and(|text| text == option.value.as_str())
            });
            if allowed {
                Ok(value.clone())
            } else {
                Err("invalid_value")
            }
        }
        FieldKind::Secret => Err("invalid_type"),
    }
}

pub fn check_fields(
    schema: &SecretTypeSchema,
    fields: &SecretFields,
) -> Result<SecretFields, Vec<FieldIssue>> {
    let mut parsed = SecretFields::new();
    let mut issues = Vec::new();
    for field in schema.config_fields() {
        match fields.get(&field.key) {
            Some(value) => match check_value(field, value) {
                Ok(value) => {
                    parsed.insert(field.key.clone(), value);
                }
                Err(code) => issues.push(FieldIssue::new(&field.key, code)),
            },
            None if field.required => issues.push(FieldIssue::new(
                &field.key,
                check_value(field, &Value::Null)
                    .err()
                    .unwrap_or("invalid_type"),
            )),
            None => {
                if let Some(default) = &field.default {
                    parsed.insert(field.key.clone(), Value::from(default));
                }
            }
        }
    }
    if fields
        .keys()
        .any(|key| !schema.config_fields().any(|field| &field.key == key))
    {
        issues.push(FieldIssue::new("", "unrecognized_keys"));
    }
    if issues.is_empty() {
        Ok(parsed)
    } else {
        Err(issues)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, json};
    use zvs_contract::{FieldDefault, FieldOption, find_secret_type_schema};

    use super::*;

    fn fields(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("an object")
    }

    fn schema(key: &str) -> &'static SecretTypeSchema {
        find_secret_type_schema(key).expect("a registry type")
    }

    #[test]
    fn defaults_apply_and_known_fields_pass_through() {
        assert_eq!(
            check_fields(schema("ollama"), &Map::new()),
            Ok(fields(json!({ "baseUrl": "https://ollama.com/api" })))
        );
        assert_eq!(
            check_fields(
                schema("openrouter"),
                &fields(json!({ "baseUrl": "  https://proxy.local/v1\n", "referer": "" }))
            ),
            Ok(fields(
                json!({ "baseUrl": "https://proxy.local/v1", "referer": "" })
            ))
        );
        assert_eq!(check_fields(schema("custom"), &Map::new()), Ok(Map::new()));
    }

    #[test]
    fn unknown_keys_the_secret_field_and_bad_values_are_issues() {
        assert_eq!(
            check_fields(schema("ollama"), &fields(json!({ "sneaky": "value" }))),
            Err(vec![FieldIssue::new("", "unrecognized_keys")])
        );
        assert_eq!(
            check_fields(schema("ollama"), &fields(json!({ "apiKey": "osk_live_x" }))),
            Err(vec![FieldIssue::new("", "unrecognized_keys")])
        );
        assert_eq!(
            check_fields(schema("ollama"), &fields(json!({ "baseUrl": "not a url" }))),
            Err(vec![FieldIssue::new("baseUrl", "invalid_format")])
        );
        assert_eq!(
            check_fields(schema("ollama"), &fields(json!({ "baseUrl": null }))),
            Err(vec![FieldIssue::new("baseUrl", "invalid_type")])
        );
        assert_eq!(
            check_fields(schema("custom"), &fields(json!({ "description": 7 }))),
            Err(vec![FieldIssue::new("description", "invalid_type")])
        );
    }

    #[test]
    fn required_selects_numbers_and_booleans_follow_their_kind() {
        let region = FieldDescriptor {
            key: "region".to_owned(),
            label: "Region".to_owned(),
            kind: FieldKind::Select,
            required: true,
            placeholder: None,
            help: None,
            options: Some(vec![FieldOption {
                value: "eu".to_owned(),
                label: "Europe".to_owned(),
            }]),
            default: None,
        };
        let mut name = region.clone();
        name.key = "name".to_owned();
        name.kind = FieldKind::Text;
        name.options = None;
        let mut timeout = name.clone();
        timeout.key = "timeout".to_owned();
        timeout.kind = FieldKind::Number;
        timeout.required = false;
        let mut insecure = timeout.clone();
        insecure.key = "insecure".to_owned();
        insecure.kind = FieldKind::Boolean;
        insecure.default = Some(FieldDefault::Boolean(false));
        let custom = SecretTypeSchema {
            key: "self-hosted".to_owned(),
            version: 1,
            label: "Self-hosted".to_owned(),
            fields: vec![region, name, timeout, insecure],
        };
        assert_eq!(
            check_fields(
                &custom,
                &fields(json!({ "region": "eu", "name": "box", "timeout": 2.5 }))
            ),
            Ok(fields(
                json!({ "region": "eu", "name": "box", "timeout": 2.5, "insecure": false })
            ))
        );
        assert_eq!(
            check_fields(
                &custom,
                &fields(json!({ "region": "us", "name": "", "insecure": "yes" }))
            ),
            Err(vec![
                FieldIssue::new("region", "invalid_value"),
                FieldIssue::new("name", "too_small"),
                FieldIssue::new("insecure", "invalid_type"),
            ])
        );
        assert_eq!(
            check_fields(&custom, &Map::new()),
            Err(vec![
                FieldIssue::new("region", "invalid_value"),
                FieldIssue::new("name", "invalid_type"),
            ])
        );
    }
}
