use rusqlite::Row;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zvs_contract::WireEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumColumn<T>(pub T);

impl<T: WireEnum> FromSql for EnumColumn<T> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        T::parse(text)
            .map(Self)
            .ok_or_else(|| FromSqlError::Other(format!("unknown value «{text}»").into()))
    }
}

impl<T: WireEnum> ToSql for EnumColumn<T> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.as_str()))
    }
}

pub fn json_column<T: DeserializeOwned>(row: &Row<'_>, name: &str) -> rusqlite::Result<T> {
    let text: String = row.get(name)?;
    let index = row.as_ref().column_index(name)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

pub fn optional_json_column<T: DeserializeOwned>(
    row: &Row<'_>,
    name: &str,
) -> rusqlite::Result<Option<T>> {
    let Some(text) = row.get::<_, Option<String>>(name)? else {
        return Ok(None);
    };
    let index = row.as_ref().column_index(name)?;
    serde_json::from_str(&text).map(Some).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

pub fn json_text<T: Serialize>(value: &T) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

pub fn qualified(table: &str, columns: &str) -> String {
    columns
        .split(", ")
        .map(|column| format!("{table}.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_qualified_one_by_one() {
        assert_eq!(
            qualified("model", "id, provider_id"),
            "model.id, model.provider_id"
        );
    }
}
