use std::sync::Arc;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, CreateScenarioInput, ScenarioDetailDto, ScenarioDto, ScenarioGraphDto,
    ScenarioId, ScenarioIssueDto, ScenarioTriggerKind, ScenarioValidationDto, ScenarioVersionDto,
    ScenarioVersionId, ToolReferenceKind, UpdateScenarioInput, normalise_tags, validate::js_trim,
};

use super::catalogue::ScenarioCatalogue;
use super::diff::diff_graphs;
use super::validation::{trigger_binding, validate_graph};
use crate::data::schema::{ScenarioRow, ScenarioVersionRow};
use crate::data::{DataError, Database, Repositories};
use crate::kernel::NodeRegistry;
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::{IdGenerator, system_ids};
use crate::services::stored::{corrupt, stored_id, unsigned};
use crate::tools::{ToolReferenceSubject, ToolRegistry};

pub const SCENARIO_NOT_FOUND: &str = "Сценарий не найден";
pub const SCENARIO_VERSION_NOT_FOUND: &str = "Версия сценария не найдена";
pub const SCENARIO_INVALID: &str = "Сценарий содержит ошибки";
pub const ACTIVE_SCENARIO_INVALID: &str = "Активный сценарий нельзя сохранить с ошибками";

pub struct ScenarioServiceOptions {
    pub database: Arc<Database>,
    pub nodes: Arc<NodeRegistry>,
    pub tools: Option<ToolRegistry>,
    pub clock: SharedClock,
    pub ids: IdGenerator,
}

impl ScenarioServiceOptions {
    pub fn new(database: Arc<Database>, nodes: Arc<NodeRegistry>) -> Self {
        Self {
            database,
            nodes,
            tools: None,
            clock: Arc::new(SystemClock),
            ids: system_ids(),
        }
    }
}

#[derive(Clone)]
pub struct ScenarioService {
    inner: Arc<ScenarioServiceOptions>,
}

fn not_found(message: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, message)
}

fn refused(message: &str, id: &ScenarioId, issues: &[ScenarioIssueDto]) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
        .with_detail("scenarioId", id.to_string())
        .with_detail("issues", json!(issues))
}

fn note_text(note: Option<&String>) -> Option<String> {
    note.map(|note| js_trim(note).to_owned())
        .filter(|note| !note.is_empty())
}

fn version_dto(row: &ScenarioVersionRow) -> Result<ScenarioVersionDto, AppError> {
    Ok(ScenarioVersionDto {
        id: stored_id(ScenarioVersionId::parse, &row.id)?,
        scenario_id: stored_id(ScenarioId::parse, &row.scenario_id)?,
        version: unsigned(row.version)?,
        note: row.note.clone(),
        summary: row.summary.clone(),
        created_at: row.created_at,
    })
}

struct Loaded {
    row: ScenarioRow,
    version: ScenarioVersionRow,
    graph: ScenarioGraphDto,
}

fn load(repositories: &Repositories<'_>, id: &str) -> Result<Loaded, DataError> {
    let row = repositories
        .scenarios
        .find_by_id(id)?
        .ok_or_else(|| not_found(SCENARIO_NOT_FOUND))?;
    let version_id = row.current_version_id.clone().ok_or_else(corrupt)?;
    let version = repositories
        .scenarios
        .find_version(&version_id)?
        .ok_or_else(corrupt)?;
    let graph = repositories
        .scenarios
        .version_graph(&version_id)?
        .ok_or_else(corrupt)?;
    Ok(Loaded {
        row,
        version,
        graph,
    })
}

impl ScenarioService {
    pub fn new(options: ScenarioServiceOptions) -> Self {
        Self {
            inner: Arc::new(options),
        }
    }

    pub fn catalogue(&self) -> ScenarioCatalogue {
        ScenarioCatalogue::from_registry(&self.inner.nodes)
    }

    pub fn validate(&self, graph: &ScenarioGraphDto) -> ScenarioValidationDto {
        let issues = validate_graph(graph, &self.catalogue());
        ScenarioValidationDto {
            valid: issues.is_empty(),
            issues,
        }
    }

    fn id(&self) -> String {
        (self.inner.ids)().hyphenated().to_string()
    }

    fn dto(&self, loaded: &Loaded, issue_count: usize) -> Result<ScenarioDto, AppError> {
        Ok(ScenarioDto {
            id: stored_id(ScenarioId::parse, &loaded.row.id)?,
            name: loaded.row.name.clone(),
            tags: loaded.row.tags.clone(),
            active: loaded.row.active,
            current_version_id: stored_id(ScenarioVersionId::parse, &loaded.version.id)?,
            version: unsigned(loaded.version.version)?,
            trigger: loaded.row.trigger.clone(),
            issue_count: issue_count as u64,
            created_at: loaded.row.created_at,
            updated_at: loaded.row.updated_at,
        })
    }

    fn detail(&self, loaded: &Loaded) -> Result<ScenarioDetailDto, AppError> {
        let issues = validate_graph(&loaded.graph, &self.catalogue());
        Ok(ScenarioDetailDto {
            scenario: self.dto(loaded, issues.len())?,
            graph: loaded.graph.clone(),
            issues,
        })
    }

    fn summary(
        &self,
        loaded: &Loaded,
        catalogue: &ScenarioCatalogue,
    ) -> Result<ScenarioDto, AppError> {
        self.dto(loaded, validate_graph(&loaded.graph, catalogue).len())
    }

    fn loaded(&self, id: &str) -> Result<Loaded, AppError> {
        self.inner
            .database
            .with_connection(|connection| load(&Repositories::new(connection), id))
    }

    pub fn list(&self) -> Result<Vec<ScenarioDto>, AppError> {
        let loaded = self.inner.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            repositories
                .scenarios
                .list()?
                .iter()
                .map(|row| load(&repositories, &row.id))
                .collect::<Result<Vec<_>, _>>()
        })?;
        let catalogue = self.catalogue();
        loaded
            .iter()
            .map(|loaded| self.summary(loaded, &catalogue))
            .collect()
    }

    pub fn get(&self, id: &ScenarioId) -> Result<ScenarioDetailDto, AppError> {
        self.detail(&self.loaded(&id.to_string())?)
    }

    pub fn create(&self, input: &CreateScenarioInput) -> Result<ScenarioDetailDto, AppError> {
        let graph = input.graph.clone().unwrap_or_default();
        let catalogue = self.catalogue();
        let now = self.inner.clock.now();
        let id = self.id();
        let version_id = self.id();
        let loaded = self.inner.database.transaction(|transaction| {
            let scenarios = Repositories::new(transaction).scenarios;
            let mut row = scenarios.save(&ScenarioRow {
                id: id.clone(),
                name: js_trim(&input.name).to_owned(),
                tags: normalise_tags(input.tags.as_deref().unwrap_or_default()),
                active: false,
                current_version_id: None,
                trigger: trigger_binding(&graph, &catalogue),
                created_at: now,
                updated_at: now,
            })?;
            let version = scenarios.add_version(
                &ScenarioVersionRow {
                    id: version_id.clone(),
                    scenario_id: id.clone(),
                    version: 1,
                    note: note_text(input.note.as_ref()),
                    summary: diff_graphs(&ScenarioGraphDto::default(), &graph),
                    created_at: now,
                },
                &graph,
            )?;
            row.current_version_id = Some(version.id.clone());
            let row = scenarios.save(&row)?;
            Ok(Loaded {
                row,
                version,
                graph: graph.clone(),
            })
        })?;
        self.record_references(&loaded, &catalogue)?;
        self.detail(&loaded)
    }

    pub fn update(&self, input: &UpdateScenarioInput) -> Result<ScenarioDetailDto, AppError> {
        let catalogue = self.catalogue();
        let now = self.inner.clock.now();
        let version_id = self.id();
        let loaded = self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let mut loaded = load(&repositories, &input.id.to_string())?;
            if let Some(name) = &input.name {
                loaded.row.name = js_trim(name).to_owned();
            }
            if let Some(tags) = &input.tags {
                loaded.row.tags = normalise_tags(tags);
            }
            if let Some(graph) = input.graph.as_ref().filter(|graph| **graph != loaded.graph) {
                let issues = validate_graph(graph, &catalogue);
                if loaded.row.active && !issues.is_empty() {
                    return Err(refused(ACTIVE_SCENARIO_INVALID, &input.id, &issues).into());
                }
                let number = repositories.scenarios.next_version(&loaded.row.id)?;
                loaded.version = repositories.scenarios.add_version(
                    &ScenarioVersionRow {
                        id: version_id.clone(),
                        scenario_id: loaded.row.id.clone(),
                        version: number,
                        note: note_text(input.note.as_ref()),
                        summary: diff_graphs(&loaded.graph, graph),
                        created_at: now,
                    },
                    graph,
                )?;
                loaded.row.current_version_id = Some(loaded.version.id.clone());
                loaded.row.trigger = trigger_binding(graph, &catalogue);
                loaded.graph = graph.clone();
            }
            loaded.row.updated_at = now;
            loaded.row = repositories.scenarios.save(&loaded.row)?;
            Ok(loaded)
        })?;
        self.record_references(&loaded, &catalogue)?;
        self.detail(&loaded)
    }

    pub fn remove(&self, id: &ScenarioId) -> Result<(), AppError> {
        let removed = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .scenarios
                .remove(&id.to_string())?)
        })?;
        if removed == 0 {
            return Err(not_found(SCENARIO_NOT_FOUND));
        }
        if let Some(tools) = &self.inner.tools {
            let id = id.to_string();
            tools.record_references(&self.subject(&id, &id), &[])?;
        }
        Ok(())
    }

    pub fn set_active(&self, id: &ScenarioId, active: bool) -> Result<ScenarioDto, AppError> {
        let catalogue = self.catalogue();
        let now = self.inner.clock.now();
        let (loaded, issues) = self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let mut loaded = load(&repositories, &id.to_string())?;
            let issues = validate_graph(&loaded.graph, &catalogue);
            if active && !issues.is_empty() {
                return Err(refused(SCENARIO_INVALID, id, &issues).into());
            }
            if loaded.row.active != active {
                loaded.row.active = active;
                loaded.row.updated_at = now;
                loaded.row = repositories.scenarios.save(&loaded.row)?;
            }
            Ok((loaded, issues))
        })?;
        self.dto(&loaded, issues.len())
    }

    pub fn versions(&self, id: &ScenarioId) -> Result<Vec<ScenarioVersionDto>, AppError> {
        let rows = self.inner.database.with_connection(|connection| {
            let scenarios = Repositories::new(connection).scenarios;
            if scenarios.find_by_id(&id.to_string())?.is_none() {
                return Err(not_found(SCENARIO_NOT_FOUND).into());
            }
            Ok(scenarios.versions(&id.to_string())?)
        })?;
        rows.iter().map(version_dto).collect()
    }

    pub fn version(
        &self,
        id: &ScenarioVersionId,
    ) -> Result<(ScenarioVersionDto, ScenarioGraphDto), AppError> {
        let (row, graph) = self.inner.database.with_connection(|connection| {
            let scenarios = Repositories::new(connection).scenarios;
            let row = scenarios
                .find_version(&id.to_string())?
                .ok_or_else(|| not_found(SCENARIO_VERSION_NOT_FOUND))?;
            let graph = scenarios.version_graph(&row.id)?.ok_or_else(corrupt)?;
            Ok((row, graph))
        })?;
        Ok((version_dto(&row)?, graph))
    }

    pub fn active_for_trigger(
        &self,
        kind: ScenarioTriggerKind,
    ) -> Result<Vec<ScenarioDto>, AppError> {
        let loaded = self.inner.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            repositories
                .scenarios
                .list_active_by_trigger(kind)?
                .iter()
                .map(|row| load(&repositories, &row.id))
                .collect::<Result<Vec<_>, _>>()
        })?;
        let catalogue = self.catalogue();
        loaded
            .iter()
            .map(|loaded| self.summary(loaded, &catalogue))
            .collect()
    }

    fn subject(&self, id: &str, name: &str) -> ToolReferenceSubject {
        ToolReferenceSubject {
            kind: ToolReferenceKind::Scenario,
            ref_id: id.to_owned(),
            name: name.to_owned(),
        }
    }

    fn record_references(
        &self,
        loaded: &Loaded,
        catalogue: &ScenarioCatalogue,
    ) -> Result<(), AppError> {
        let Some(tools) = &self.inner.tools else {
            return Ok(());
        };
        let mut keys: Vec<&str> = Vec::new();
        for node in &loaded.graph.nodes {
            if let Some(key) = catalogue
                .spec(&node.node_type)
                .and_then(|spec| spec.tool_key.as_deref())
                && !keys.contains(&key)
            {
                keys.push(key);
            }
        }
        tools.record_references(&self.subject(&loaded.row.id, &loaded.row.name), &keys)
    }
}
