use std::sync::Arc;

use serde_json::Value;
use zvs_contract::{AppError, AppErrorCode, Json, SettingsSetOutput, Timestamp};

use crate::data::schema::SettingRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;

pub type SettingDto = SettingsSetOutput;

#[derive(Clone)]
pub struct SettingService {
    database: Arc<Database>,
    clock: SharedClock,
}

impl SettingService {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self { database, clock }
    }

    pub fn get(&self, key: &str) -> Result<Option<SettingDto>, AppError> {
        self.database
            .with_connection(|connection| Ok(Repositories::new(connection).settings.get(key)?))?
            .map(to_dto)
            .transpose()
    }

    pub fn list(&self) -> Result<Vec<SettingDto>, AppError> {
        self.database
            .with_connection(|connection| Ok(Repositories::new(connection).settings.list()?))?
            .into_iter()
            .map(to_dto)
            .collect()
    }

    pub fn set(&self, key: &str, value: &Json) -> Result<SettingDto, AppError> {
        let encoded = value.to_string();
        let updated_at = self.now();
        to_dto(self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .settings
                .set(key, &encoded, updated_at)?)
        })?)
    }

    pub fn set_many(&self, entries: &[(&str, Json)]) -> Result<Vec<SettingDto>, AppError> {
        let encoded: Vec<(&str, String)> = entries
            .iter()
            .map(|(key, value)| (*key, value.to_string()))
            .collect();
        let updated_at = self.now();
        self.database
            .transaction(|transaction| {
                let repositories = Repositories::new(transaction);
                encoded
                    .iter()
                    .map(|(key, value)| Ok(repositories.settings.set(key, value, updated_at)?))
                    .collect::<Result<Vec<_>, _>>()
            })?
            .into_iter()
            .map(to_dto)
            .collect()
    }

    pub fn remove(&self, key: &str) -> Result<(), AppError> {
        self.database
            .with_connection(|connection| Ok(Repositories::new(connection).settings.remove(key)?))
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

fn to_dto(row: SettingRow) -> Result<SettingDto, AppError> {
    let value: Value = serde_json::from_str(&row.value).map_err(|_| {
        AppError::new(
            AppErrorCode::DbError,
            "Настройка хранится в повреждённом виде",
        )
        .with_detail("key", row.key.clone())
    })?;
    Ok(SettingDto {
        key: row.key,
        value,
        updated_at: row.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::{CLOCK_ORIGIN, temporary_database};

    fn service(database: &crate::testing::TemporaryDatabase) -> SettingService {
        SettingService::new(database.shared(), Arc::new(FakeClock::default()))
    }

    #[test]
    fn values_round_trip_as_json_with_the_clock_timestamp() {
        let database = temporary_database();
        let settings = service(&database);
        assert_eq!(settings.get("ui.theme").expect("reads"), None);
        let geometry = json!({ "bounds": { "x": 10, "y": 20, "width": 1600, "height": 1000 }, "maximized": false });
        let written = settings.set("window.main", &geometry).expect("writes");
        assert_eq!(
            written,
            SettingDto {
                key: "window.main".to_owned(),
                value: geometry,
                updated_at: CLOCK_ORIGIN,
            }
        );
        assert_eq!(settings.get("window.main").expect("reads"), Some(written));
        settings.set("ui.flag", &Value::Null).expect("writes null");
        assert_eq!(
            settings.get("ui.flag").expect("reads").map(|dto| dto.value),
            Some(Value::Null)
        );
        assert_eq!(
            settings
                .list()
                .expect("lists")
                .into_iter()
                .map(|dto| dto.key)
                .collect::<Vec<_>>(),
            ["ui.flag", "window.main"]
        );
        settings.remove("ui.flag").expect("removes");
        assert_eq!(settings.get("ui.flag").expect("reads"), None);
    }

    #[test]
    fn a_transaction_rolls_the_whole_unit_of_work_back_when_it_fails() {
        let database = temporary_database();
        let settings = service(&database);
        settings.set("theme.name", &json!("dark")).expect("writes");

        let failed = database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            repositories.settings.set("theme.name", "\"light\"", 2)?;
            repositories.settings.set("locale.name", "\"ru\"", 2)?;
            Err::<(), _>(
                AppError::new(AppErrorCode::Unknown, "the unit of work failed halfway").into(),
            )
        });
        assert!(failed.is_err());
        assert_eq!(
            settings
                .get("theme.name")
                .expect("reads")
                .map(|dto| dto.value),
            Some(json!("dark"))
        );
        assert_eq!(settings.get("locale.name").expect("reads"), None);

        settings
            .set_many(&[("theme.name", json!("light")), ("locale.name", json!("ru"))])
            .expect("writes both");
        assert_eq!(
            settings
                .get("theme.name")
                .expect("reads")
                .map(|dto| dto.value),
            Some(json!("light"))
        );
        assert_eq!(
            settings
                .get("locale.name")
                .expect("reads")
                .map(|dto| dto.value),
            Some(json!("ru"))
        );
    }

    #[test]
    fn a_corrupt_stored_value_is_a_db_error_naming_the_key() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .settings
                    .set("ui.theme", "{not json", 1)?)
            })
            .expect("writes raw");
        let error = service(&database).get("ui.theme").expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::DbError);
        assert_eq!(error.message, "Настройка хранится в повреждённом виде");
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("key").cloned()),
            Some(json!("ui.theme"))
        );
    }
}
