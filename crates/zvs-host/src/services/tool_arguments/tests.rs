use serde_json::json;
use zvs_contract::validate::invalid;

use super::*;
use crate::commands::nodes::ShellRunInput;
use crate::files::nodes::FsApplyInput;
use crate::kernel::input_schema;

fn validation() -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, VALIDATION_MESSAGE)
}

#[test]
fn a_json_array_sent_as_a_string_names_the_field_and_the_fix() {
    let schema = input_schema::<ShellRunInput>();
    let arguments = json!({
        "argv": "[\"cmd.exe\", \"/c\", \"start\", \"\", \"C:\\\\report.docx\"]",
        "reason": "Открыть шаблон отчёта"
    });

    let problems = check(&schema, &arguments);

    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path, "argv");
    assert!(problems[0].text.contains(type_label("array")));
    assert!(problems[0].text.contains(type_label("string")));
    assert!(problems[0].text.contains(JSON_IN_STRING));
}

#[test]
fn the_failure_text_lists_problems_the_expected_shape_and_a_retry_hint() {
    let schema = input_schema::<ShellRunInput>();
    let arguments = json!({ "argv": "npm test" });

    let text = arguments_failure(&validation(), &schema, &arguments).expect("a failure text");

    assert!(text.starts_with(ARGUMENTS_LEAD));
    assert!(text.contains(&format!("- reason: {MISSING_FIELD}")));
    assert!(text.contains("- argv: "));
    assert!(!text.contains(JSON_IN_STRING));
    assert!(text.contains(&format!("{SHAPE_LEAD} {}", shape(&schema))));
    assert!(text.ends_with(RETRY_HINT));
}

#[test]
fn the_expected_shape_holds_the_required_fields() {
    let schema = input_schema::<ShellRunInput>();

    assert_eq!(shape(&schema), json!({ "argv": ["…"], "reason": "…" }));
}

#[test]
fn a_rule_failure_falls_back_to_the_field_and_the_rule() {
    let schema = input_schema::<ShellRunInput>();
    let arguments = json!({ "argv": ["npm"], "reason": "   " });

    let text = arguments_failure(&invalid("reason", "nonEmpty"), &schema, &arguments)
        .expect("a failure text");

    assert!(text.contains(&format!("- reason: {RULE_LEAD} nonEmpty")));
}

#[test]
fn a_serde_reason_is_kept_when_the_schema_has_nothing_to_say() {
    let error = validation().with_detail("reason", "invalid value: integer `-1`");

    let text = arguments_failure(&error, &json!({}), &json!({})).expect("a failure text");

    assert!(text.contains(&format!("{REASON_LEAD} invalid value: integer `-1`")));
}

#[test]
fn a_specific_validation_message_is_left_alone() {
    let error = AppError::new(AppErrorCode::ValidationFailed, "Путь не найден");

    assert_eq!(arguments_failure(&error, &json!({}), &json!({})), None);
}

#[test]
fn other_errors_are_left_alone() {
    let error = AppError::new(AppErrorCode::NotFound, "Нет файла");

    assert_eq!(arguments_failure(&error, &json!({}), &json!({})), None);
}

#[test]
fn a_tagged_variant_with_an_unknown_tag_lists_the_allowed_tags() {
    let schema = input_schema::<FsApplyInput>();
    let arguments = json!({ "changes": [{ "op": "wrte", "path": "a.txt", "content": "" }] });

    let problems = check(&schema, &arguments);

    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path, "changes[0].op");
    assert!(problems[0].text.contains("\"write\""));
    assert!(problems[0].text.contains("\"delete\""));
}

#[test]
fn a_tagged_variant_reports_its_own_missing_field() {
    let schema = input_schema::<FsApplyInput>();
    let arguments = json!({ "changes": [{ "op": "write", "path": "a.txt" }] });

    let problems = check(&schema, &arguments);

    assert_eq!(
        problems,
        vec![Problem {
            path: "changes[0].content".to_owned(),
            text: MISSING_FIELD.to_owned(),
        }]
    );
}

#[test]
fn valid_arguments_have_no_problems() {
    let schema = input_schema::<FsApplyInput>();
    let arguments = json!({ "changes": [
        { "op": "write", "path": "a.txt", "content": "text" },
        { "op": "edit", "path": "b.txt", "edits": [{ "old": "x", "new": "y" }] }
    ] });

    assert!(check(&schema, &arguments).is_empty());
}

#[test]
fn enums_and_item_counts_are_checked() {
    let schema = json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string", "enum": ["text", "single"] },
            "names": { "type": "array", "minItems": 1, "items": { "type": "string" } }
        }
    });

    let problems = check(&schema, &json!({ "kind": "other", "names": [] }));

    assert_eq!(problems.len(), 2);
    assert_eq!(problems[0].path, "kind");
    assert_eq!(problems[1].path, "names");
}

#[test]
fn refs_resolve_against_the_root() {
    let schema = json!({
        "type": "object",
        "properties": { "point": { "$ref": "#/$defs/point" } },
        "$defs": { "point": { "type": "object", "required": ["x"] } }
    });

    let problems = check(&schema, &json!({ "point": {} }));

    assert_eq!(problems[0].path, "point.x");
}

#[test]
fn integers_accept_whole_floats() {
    let schema = json!({ "type": "integer" });

    assert!(check(&schema, &json!(3.0)).is_empty());
    assert_eq!(check(&schema, &json!(3.5))[0].path, ARGUMENTS_ROOT);
}

#[test]
fn computer_actions_expect_an_array_and_show_an_executable_example() {
    use crate::computer::nodes::{ACTIONS_MAX, ActInput};

    let schema = input_schema::<ActInput>();
    let actions = &schema["properties"]["actions"];
    assert_eq!(actions["type"], "array");
    assert_eq!(actions["minItems"], 1);
    assert_eq!(actions["maxItems"], ACTIONS_MAX);

    let expected = json!({"actions": [
        {"type": "click", "x": 32, "y": 288},
        {"type": "wait", "ms": 500}
    ]});
    assert_eq!(shape(&schema), expected);
    assert!(check(&schema, &expected).is_empty());
    zvs_contract::parse::<ActInput>(expected.clone()).expect("a valid computer.act example");

    let arguments = json!({"actions": expected["actions"].to_string()});
    let problems = check(&schema, &arguments);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path, "actions");
    assert!(problems[0].text.contains(JSON_IN_STRING));
    assert!(zvs_contract::parse::<ActInput>(arguments.clone()).is_err());

    let text = arguments_failure(&validation(), &schema, &arguments).expect("a failure text");
    assert!(text.contains(&format!("{SHAPE_LEAD} {expected}")));
}

#[test]
fn the_computer_action_example_survives_the_model_description_limit() {
    use crate::computer::nodes::ACT_DESCRIPTION;
    use crate::services::chat_tools::TOOL_DESCRIPTION_MAX;

    assert!(ACT_DESCRIPTION.encode_utf16().count() <= TOOL_DESCRIPTION_MAX);
    assert_eq!(examples(ACT_DESCRIPTION).len(), 1);
}

fn examples(description: &str) -> Vec<Value> {
    description
        .match_indices("arguments: {")
        .chain(description.match_indices("Windows: {"))
        .filter_map(|(at, lead)| {
            let rest = &description[at + lead.len() - 1..];
            serde_json::Deserializer::from_str(rest)
                .into_iter::<Value>()
                .next()?
                .ok()
        })
        .collect()
}

#[test]
fn every_description_example_matches_its_schema() {
    use crate::commands::nodes::SHELL_DESCRIPTION;
    use crate::computer::nodes::{
        ACT_DESCRIPTION, AccessInput, ActInput, REQUEST_ACCESS_DESCRIPTION,
    };
    use crate::files::nodes::{
        APPLY_DESCRIPTION, FsReadInput, READ_DESCRIPTION, READ_IMAGE_DESCRIPTION,
    };
    use crate::services::chat_ask::{ASK_DESCRIPTION, ask_parameters};

    let cases = [
        (SHELL_DESCRIPTION, input_schema::<ShellRunInput>(), 2),
        (ACT_DESCRIPTION, input_schema::<ActInput>(), 1),
        (REQUEST_ACCESS_DESCRIPTION, input_schema::<AccessInput>(), 1),
        (READ_DESCRIPTION, input_schema::<FsReadInput>(), 1),
        (READ_IMAGE_DESCRIPTION, input_schema::<FsReadInput>(), 1),
        (APPLY_DESCRIPTION, input_schema::<FsApplyInput>(), 1),
        (ASK_DESCRIPTION, ask_parameters(), 1),
    ];
    for (description, schema, count) in cases {
        let found = examples(description);
        assert_eq!(found.len(), count, "{description}");
        for example in found {
            assert_eq!(check(&schema, &example), Vec::new(), "{example}");
        }
    }
}

#[test]
fn description_examples_parse_into_the_typed_inputs() {
    use crate::commands::nodes::SHELL_DESCRIPTION;
    use crate::computer::nodes::{ACT_DESCRIPTION, ActInput};
    use crate::files::nodes::APPLY_DESCRIPTION;

    for example in examples(SHELL_DESCRIPTION) {
        zvs_contract::parse::<ShellRunInput>(example).expect("a valid shell.run call");
    }
    for example in examples(ACT_DESCRIPTION) {
        zvs_contract::parse::<ActInput>(example).expect("a valid computer.act call");
    }
    for example in examples(APPLY_DESCRIPTION) {
        zvs_contract::parse::<FsApplyInput>(example).expect("a valid fs.apply call");
    }
}

#[test]
fn a_json_array_sent_as_a_string_is_repaired_to_the_array() {
    let schema = input_schema::<ShellRunInput>();
    let arguments = json!({ "argv": " [\"npm\", \"test\"] ", "reason": "Тесты" });

    let repaired = repair(&schema, arguments);

    assert_eq!(
        repaired,
        json!({ "argv": ["npm", "test"], "reason": "Тесты" })
    );
    assert!(check(&schema, &repaired).is_empty());
    zvs_contract::parse::<ShellRunInput>(repaired).expect("a valid shell.run call");
}

#[test]
fn a_string_field_holding_json_stays_a_string() {
    let schema = input_schema::<FsApplyInput>();
    let arguments = json!({ "changes": [
        { "op": "write", "path": "a.json", "content": "{\"a\": [1]}" }
    ] });

    assert_eq!(repair(&schema, arguments.clone()), arguments);
}

#[test]
fn a_string_that_is_not_json_or_has_the_wrong_type_is_left_for_the_checker() {
    let schema = input_schema::<ShellRunInput>();
    let plain = json!({ "argv": "npm test", "reason": "Тесты" });
    let wrong = json!({ "argv": "{\"program\": \"npm\"}", "reason": "Тесты" });
    let broken = json!({ "argv": "[\"npm\", ", "reason": "Тесты" });

    assert_eq!(repair(&schema, plain.clone()), plain);
    assert_eq!(repair(&schema, wrong.clone()), wrong);
    assert_eq!(repair(&schema, broken.clone()), broken);
}

#[test]
fn nested_strings_inside_tagged_variants_are_repaired() {
    let schema = input_schema::<FsApplyInput>();
    let edits = json!([{ "old": "a", "new": "b" }]).to_string();
    let changes = json!([
        { "op": "edit", "path": "src/app.ts", "edits": edits },
        { "op": "write", "path": "notes.md", "content": "[x]" }
    ])
    .to_string();

    let repaired = repair(&schema, json!({ "changes": changes }));

    assert_eq!(
        repaired,
        json!({ "changes": [
            { "op": "edit", "path": "src/app.ts", "edits": [{ "old": "a", "new": "b" }] },
            { "op": "write", "path": "notes.md", "content": "[x]" }
        ] })
    );
    assert!(check(&schema, &repaired).is_empty());
}

#[test]
fn computer_actions_sent_as_a_string_are_repaired() {
    use crate::computer::nodes::ActInput;

    let schema = input_schema::<ActInput>();
    let actions = json!([{"type": "click", "x": 32, "y": 288}, {"type": "wait", "ms": 500}]);

    let repaired = repair(&schema, json!({ "actions": actions.to_string() }));

    assert_eq!(repaired, json!({ "actions": actions }));
    zvs_contract::parse::<ActInput>(repaired).expect("a valid computer.act call");
}

#[test]
fn arguments_sent_as_one_json_string_are_unwrapped() {
    use crate::services::chat_tools::{parse_tool_arguments, parse_tool_call};

    let schema = input_schema::<ShellRunInput>();
    let raw = json!(json!({ "argv": "[\"npm\"]", "reason": "Тесты" }).to_string()).to_string();

    assert_eq!(
        parse_tool_arguments(&raw).map(Value::Object),
        Some(json!({ "argv": "[\"npm\"]", "reason": "Тесты" }))
    );
    assert_eq!(
        parse_tool_call(&raw, &schema).map(Value::Object),
        Some(json!({ "argv": ["npm"], "reason": "Тесты" }))
    );
    assert_eq!(parse_tool_arguments("\"npm test\""), None);
}
