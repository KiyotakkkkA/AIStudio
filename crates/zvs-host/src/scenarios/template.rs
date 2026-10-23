use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::drivers::ai::adapters::wire::stringify;
use crate::kernel::project;

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([A-Za-z]+)((?:\.[A-Za-z0-9_\-]+)*)\s*\}\}").expect("a valid pattern")
});

pub const RESULT_ROOT: &str = "result";
pub const EVENT_ROOT: &str = "event";
pub const DATE_ROOT: &str = "date";

pub fn text_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Object(map) => match map.get("text") {
            Some(Value::String(text)) => text.clone(),
            _ => stringify(value),
        },
        other => stringify(other),
    }
}

pub struct TemplateScope<'a> {
    pub result: &'a Value,
    pub event: &'a Value,
    pub date: &'a str,
}

pub fn render(template: &str, scope: &TemplateScope<'_>) -> String {
    PLACEHOLDER
        .replace_all(template, |captures: &regex::Captures<'_>| {
            let path = captures
                .get(2)
                .map_or("", |path| path.as_str().trim_start_matches('.'));
            match &captures[1] {
                RESULT_ROOT => text_of(&project(scope.result, path)),
                EVENT_ROOT => text_of(&project(scope.event, path)),
                DATE_ROOT if path.is_empty() => scope.date.to_owned(),
                _ => captures[0].to_owned(),
            }
        })
        .into_owned()
}

pub fn civil_date(timestamp_ms: i64) -> String {
    let days = timestamp_ms.div_euclid(86_400_000);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{day:02}.{month:02}.{year}")
}
