use crate::data::{Database, Repositories};
use serde_json::Value;
use zvs_contract::AppError;

pub const COMPUTER_ENABLED_KEY: &str = "computer.enabled";

pub fn command_enabled(database: &Database, key: &str) -> Result<bool, AppError> {
    let setting_key = match key {
        "computer-use" => COMPUTER_ENABLED_KEY,
        "browser-use" => crate::browser::settings::BROWSER_SETTINGS_KEY,
        _ => return Ok(true),
    };
    database.with_connection(|connection| {
        let stored = Repositories::new(connection)
            .settings
            .get(setting_key)?
            .and_then(|row| serde_json::from_str::<Value>(&row.value).ok());
        Ok(if key == "browser-use" {
            crate::browser::settings::decode_settings(stored.as_ref()).enabled
        } else {
            stored.as_ref().and_then(Value::as_bool).unwrap_or(true)
        })
    })
}
