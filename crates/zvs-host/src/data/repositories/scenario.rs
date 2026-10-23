use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::{ScenarioGraphDto, ScenarioTriggerKind};

use crate::data::schema::columns::{EnumColumn, json_text};
use crate::data::schema::{ScenarioRow, ScenarioVersionRow};

pub struct ScenarioRepository<'c> {
    connection: &'c Connection,
}

impl<'c> ScenarioRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<ScenarioRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM scenario ORDER BY updated_at DESC, id",
                ScenarioRow::COLUMNS
            ))?
            .query_map([], ScenarioRow::from_row)?
            .collect()
    }

    pub fn list_active_by_trigger(
        &self,
        kind: ScenarioTriggerKind,
    ) -> rusqlite::Result<Vec<ScenarioRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM scenario WHERE active = 1 AND trigger_kind = ?1 ORDER BY id",
                ScenarioRow::COLUMNS
            ))?
            .query_map([EnumColumn(kind)], ScenarioRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<ScenarioRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM scenario WHERE id = ?1",
                ScenarioRow::COLUMNS
            ))?
            .query_row([id], ScenarioRow::from_row)
            .optional()
    }

    pub fn save(&self, row: &ScenarioRow) -> rusqlite::Result<ScenarioRow> {
        let trigger = row.trigger.as_ref().map(json_text).transpose()?;
        let trigger_kind = row.trigger.as_ref().map(|trigger| EnumColumn(trigger.kind));
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO scenario ({columns}, trigger_kind) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name, tags = excluded.tags, \
                 active = excluded.active, current_version_id = excluded.current_version_id, \
                 trigger = excluded.trigger, trigger_kind = excluded.trigger_kind, \
                 updated_at = excluded.updated_at \
                 RETURNING {columns}",
                columns = ScenarioRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.name,
                    json_text(&row.tags)?,
                    row.active,
                    row.current_version_id,
                    trigger,
                    row.created_at,
                    row.updated_at,
                    trigger_kind,
                ],
                ScenarioRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM scenario WHERE id = ?1")?
            .execute([id])
    }

    pub fn next_version(&self, scenario_id: &str) -> rusqlite::Result<i64> {
        self.connection
            .prepare_cached(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM scenario_version WHERE scenario_id = ?1",
            )?
            .query_row([scenario_id], |row| row.get(0))
    }

    pub fn add_version(
        &self,
        row: &ScenarioVersionRow,
        graph: &ScenarioGraphDto,
    ) -> rusqlite::Result<ScenarioVersionRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO scenario_version (id, scenario_id, version, graph, note, summary, \
                 created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING {}",
                ScenarioVersionRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.scenario_id,
                    row.version,
                    json_text(graph)?,
                    row.note,
                    json_text(&row.summary)?,
                    row.created_at,
                ],
                ScenarioVersionRow::from_row,
            )
    }

    pub fn versions(&self, scenario_id: &str) -> rusqlite::Result<Vec<ScenarioVersionRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM scenario_version WHERE scenario_id = ?1 ORDER BY version DESC",
                ScenarioVersionRow::COLUMNS
            ))?
            .query_map([scenario_id], ScenarioVersionRow::from_row)?
            .collect()
    }

    pub fn find_version(&self, id: &str) -> rusqlite::Result<Option<ScenarioVersionRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM scenario_version WHERE id = ?1",
                ScenarioVersionRow::COLUMNS
            ))?
            .query_row([id], ScenarioVersionRow::from_row)
            .optional()
    }

    pub fn version_graph(&self, id: &str) -> rusqlite::Result<Option<ScenarioGraphDto>> {
        self.connection
            .prepare_cached("SELECT graph FROM scenario_version WHERE id = ?1")?
            .query_row([id], |row| {
                crate::data::schema::columns::json_column(row, "graph")
            })
            .optional()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Map;
    use zvs_contract::{
        ScenarioAttachmentDto, ScenarioDiffDto, ScenarioNodeDto, ScenarioPositionDto, ScenarioSlot,
        ScenarioTriggerDto,
    };

    use super::*;
    use crate::data::Repositories;
    use crate::testing::temporary_database;

    fn scenario(id: &str, trigger: Option<ScenarioTriggerKind>) -> ScenarioRow {
        ScenarioRow {
            id: id.to_owned(),
            name: "Триаж".to_owned(),
            tags: vec!["support".to_owned()],
            active: false,
            current_version_id: None,
            trigger: trigger.map(|kind| ScenarioTriggerDto {
                kind,
                node_id: "trigger".to_owned(),
                params: Map::new(),
            }),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn version(id: &str, scenario_id: &str, version: i64) -> ScenarioVersionRow {
        ScenarioVersionRow {
            id: id.to_owned(),
            scenario_id: scenario_id.to_owned(),
            version,
            note: None,
            summary: ScenarioDiffDto::default(),
            created_at: version,
        }
    }

    fn node(id: &str, node_type: &str) -> ScenarioNodeDto {
        ScenarioNodeDto {
            id: id.to_owned(),
            node_type: node_type.to_owned(),
            name: None,
            position: ScenarioPositionDto { x: 0.5, y: 1.5 },
            params: Map::new(),
            disabled: None,
        }
    }

    #[test]
    fn versions_number_upwards_keep_their_graph_and_go_with_the_scenario() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let scenarios = Repositories::new(connection).scenarios;
                scenarios.save(&scenario("s", None))?;
                assert_eq!(scenarios.next_version("s")?, 1);
                let graph = ScenarioGraphDto {
                    nodes: vec![node("agent", "agent.tools"), node("model", "model.chat")],
                    edges: Vec::new(),
                    attachments: vec![ScenarioAttachmentDto {
                        parent_node_id: "agent".to_owned(),
                        child_node_id: "model".to_owned(),
                        slot: ScenarioSlot::Model,
                    }],
                };
                scenarios.add_version(&version("v1", "s", 1), &ScenarioGraphDto::default())?;
                scenarios.add_version(&version("v2", "s", 2), &graph)?;
                assert_eq!(scenarios.next_version("s")?, 3);
                assert!(
                    scenarios
                        .add_version(&version("v3", "s", 2), &graph)
                        .is_err()
                );
                assert_eq!(
                    scenarios
                        .versions("s")?
                        .into_iter()
                        .map(|row| row.version)
                        .collect::<Vec<_>>(),
                    [2, 1]
                );
                assert_eq!(scenarios.version_graph("v2")?, Some(graph));
                assert_eq!(
                    scenarios.find_version("v1")?.map(|row| row.version),
                    Some(1)
                );
                assert_eq!(scenarios.remove("s")?, 1);
                assert!(scenarios.versions("s")?.is_empty());
                assert_eq!(scenarios.version_graph("v2")?, None);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn an_update_keeps_the_creation_time_and_the_trigger_binding_is_queryable() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let scenarios = Repositories::new(connection).scenarios;
                scenarios.save(&scenario("chat", Some(ScenarioTriggerKind::Chat)))?;
                scenarios.save(&scenario("mail", Some(ScenarioTriggerKind::Mail)))?;
                scenarios.save(&scenario("none", None))?;
                assert!(
                    scenarios
                        .list_active_by_trigger(ScenarioTriggerKind::Chat)?
                        .is_empty()
                );
                let saved = scenarios.save(&ScenarioRow {
                    active: true,
                    created_at: 99,
                    updated_at: 5,
                    ..scenario("chat", Some(ScenarioTriggerKind::Chat))
                })?;
                assert_eq!(saved.created_at, 1);
                assert_eq!(saved.updated_at, 5);
                assert_eq!(
                    scenarios
                        .list_active_by_trigger(ScenarioTriggerKind::Chat)?
                        .into_iter()
                        .map(|row| row.id)
                        .collect::<Vec<_>>(),
                    ["chat"]
                );
                assert_eq!(
                    scenarios.find_by_id("chat")?.and_then(|row| row.trigger),
                    saved.trigger
                );
                assert_eq!(scenarios.list()?[0].id, "chat");
                Ok(())
            })
            .expect("the repository works");
    }
}
