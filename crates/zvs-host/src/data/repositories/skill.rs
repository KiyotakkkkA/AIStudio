use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::SkillRow;
use crate::data::schema::columns::{EnumColumn, json_text};

pub struct SkillRepository<'c> {
    connection: &'c Connection,
}

impl<'c> SkillRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<SkillRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM skill ORDER BY key",
                SkillRow::COLUMNS
            ))?
            .query_map([], SkillRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<SkillRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM skill WHERE id = ?1",
                SkillRow::COLUMNS
            ))?
            .query_row([id], SkillRow::from_row)
            .optional()
    }

    pub fn find_by_key(&self, key: &str) -> rusqlite::Result<Option<SkillRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM skill WHERE key = ?1",
                SkillRow::COLUMNS
            ))?
            .query_row([key], SkillRow::from_row)
            .optional()
    }

    pub fn save(&self, row: &SkillRow) -> rusqlite::Result<SkillRow> {
        let model_override = row.model_override.as_ref().map(json_text).transpose()?;
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO skill ({columns}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17) \
                 ON CONFLICT (id) DO UPDATE SET key = excluded.key, name = excluded.name, \
                 description = excluded.description, allowed_tools = excluded.allowed_tools, \
                 model_override = excluded.model_override, enabled = excluded.enabled, \
                 source = excluded.source, status = excluded.status, issues = excluded.issues, \
                 file_count = excluded.file_count, content_hash = excluded.content_hash, \
                 path = excluded.path, version = excluded.version, author = excluded.author, \
                 updated_at = excluded.updated_at \
                 RETURNING {columns}",
                columns = SkillRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.key,
                    row.name,
                    row.description,
                    json_text(&row.allowed_tools)?,
                    model_override,
                    row.enabled,
                    EnumColumn(row.source),
                    EnumColumn(row.status),
                    json_text(&row.issues)?,
                    row.file_count,
                    row.content_hash,
                    row.path,
                    row.version,
                    row.author,
                    row.created_at,
                    row.updated_at,
                ],
                SkillRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM skill WHERE id = ?1")?
            .execute([id])
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::{SkillIssueDto, SkillIssueRule, SkillSource, SkillStatus};

    use super::*;
    use crate::data::Repositories;
    use crate::testing::temporary_database;

    fn row(id: &str, key: &str) -> SkillRow {
        SkillRow {
            id: id.to_owned(),
            key: key.to_owned(),
            name: "Навык".to_owned(),
            description: "Описание".to_owned(),
            allowed_tools: vec!["vector.search".to_owned()],
            model_override: None,
            enabled: true,
            source: SkillSource::Local,
            status: SkillStatus::Ready,
            issues: Vec::new(),
            file_count: 0,
            content_hash: "h".to_owned(),
            path: format!("C:/skills/{key}"),
            version: None,
            author: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn skills_are_saved_listed_by_key_updated_in_place_and_removed() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let skills = Repositories::new(connection).skills;
                skills.save(&row("b", "beta"))?;
                skills.save(&row("a", "alpha"))?;
                assert_eq!(
                    skills
                        .list()?
                        .into_iter()
                        .map(|row| row.key)
                        .collect::<Vec<_>>(),
                    ["alpha", "beta"]
                );
                let changed = skills.save(&SkillRow {
                    status: SkillStatus::Invalid,
                    issues: vec![SkillIssueDto {
                        field: "allowedTools.0".to_owned(),
                        rule: SkillIssueRule::ToolOff,
                    }],
                    updated_at: 5,
                    created_at: 99,
                    ..row("a", "alpha")
                })?;
                assert_eq!(changed.status, SkillStatus::Invalid);
                assert_eq!(changed.created_at, 1);
                assert_eq!(changed.updated_at, 5);
                assert_eq!(
                    skills.find_by_key("alpha")?.map(|row| row.id),
                    Some("a".to_owned())
                );
                assert!(skills.save(&row("c", "alpha")).is_err());
                assert_eq!(skills.remove("a")?, 1);
                assert_eq!(skills.find_by_id("a")?, None);
                Ok(())
            })
            .expect("the repository works");
    }
}
