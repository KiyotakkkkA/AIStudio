use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use zvs_contract::validate::{check, invalid, js_trim};
use zvs_contract::{
    AppError, AppErrorCode, Timestamp, ToolDefaultTier, ToolDefaultsDto, ToolDetailDto, ToolDto,
    ToolId, ToolListFilter, ToolReferenceDto, ToolReferenceKind, ToolSource, ToolTier,
    ToolUsageDto, Validate, ids::is_wire_uuid, tools::check_tool_key,
};

use crate::data::repositories::ToolPatch;
use crate::data::schema::{ToolPermissionRow, ToolReferenceRow, ToolRow};
use crate::data::{DataError, Database, Repositories};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::SettingService;
use crate::services::permissions::GLOBAL_SCOPE;

pub const TOOL_DEFAULTS_KEY: &str = "tools.defaults";
pub const TOOL_NOT_FOUND: &str = "Инструмент не найден";

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDescriptor {
    pub key: String,
    pub source: ToolSource,
    pub source_ref: Option<String>,
    pub display_name: Option<String>,
    pub description: String,
    pub input_schema: Value,
    pub read_only: bool,
    pub destructive: bool,
    pub tier: Option<ToolTier>,
}

impl ToolDescriptor {
    pub fn new(key: impl Into<String>, source: ToolSource) -> Self {
        Self {
            key: key.into(),
            source,
            source_ref: None,
            display_name: None,
            description: String::new(),
            input_schema: Value::Null,
            read_only: false,
            destructive: false,
            tier: None,
        }
    }

    fn normalized(&self) -> Result<Self, AppError> {
        check_tool_key("key", &self.key)?;
        let source_ref = self
            .source_ref
            .as_deref()
            .map(|value| {
                let trimmed = js_trim(value);
                check::non_empty("sourceRef", trimmed)?;
                check::max_len("sourceRef", trimmed, 256)?;
                Ok::<_, AppError>(trimmed.to_owned())
            })
            .transpose()?;
        let display_name = self
            .display_name
            .as_deref()
            .map(|value| {
                check::trimmed_name("displayName", value, 200)?;
                Ok::<_, AppError>(js_trim(value).to_owned())
            })
            .transpose()?;
        let description = js_trim(&self.description).to_owned();
        check::max_len("description", &description, 4000)?;
        Ok(Self {
            key: js_trim(&self.key).to_owned(),
            source: self.source,
            source_ref,
            display_name,
            description,
            input_schema: self.input_schema.clone(),
            read_only: self.read_only,
            destructive: self.destructive,
            tier: self.tier,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReferenceSubject {
    pub kind: ToolReferenceKind,
    pub ref_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolCeiling {
    pub tier: ToolTier,
    pub enabled: bool,
    pub available: bool,
}

pub const fn default_tool_tier(
    read_only: bool,
    destructive: bool,
    preference: ToolDefaultTier,
) -> ToolTier {
    if destructive {
        return ToolTier::Off;
    }
    match preference {
        ToolDefaultTier::Auto => ToolTier::Auto,
        ToolDefaultTier::Ask => ToolTier::Ask,
        ToolDefaultTier::Off => ToolTier::Off,
        ToolDefaultTier::Inferred if read_only => ToolTier::Auto,
        ToolDefaultTier::Inferred => ToolTier::Ask,
    }
}

pub struct ToolRegistryOptions {
    pub database: Arc<Database>,
    pub settings: Option<SettingService>,
    pub logger: SharedLogger,
    pub clock: SharedClock,
}

impl ToolRegistryOptions {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            settings: None,
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
        }
    }
}

struct Inner {
    database: Arc<Database>,
    settings: Option<SettingService>,
    logger: SharedLogger,
    clock: SharedClock,
    defaults: Mutex<ToolDefaultsDto>,
}

#[derive(Clone)]
pub struct ToolRegistry {
    inner: Arc<Inner>,
}

fn id_text() -> String {
    new_id().hyphenated().to_string()
}

fn not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, TOOL_NOT_FOUND)
}

fn malformed() -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        "Инструмент хранится в повреждённом виде",
    )
}

pub fn tool_dto(row: &ToolRow) -> Result<ToolDto, AppError> {
    let dto = ToolDto {
        id: ToolId::parse(&row.id).ok_or_else(malformed)?,
        key: row.key.clone(),
        source: row.source,
        source_ref: row.source_ref.clone(),
        display_name: row.display_name.clone(),
        description: row.description.clone(),
        input_schema: row.input_schema.clone(),
        read_only: row.read_only,
        destructive: row.destructive,
        tier: row.tier,
        enabled: row.enabled,
        available: row.available,
        last_used_at: row.last_used_at,
        call_count: row.call_count,
        registered_at: row.registered_at,
        updated_at: row.updated_at,
    };
    dto.validate()?;
    Ok(dto)
}

fn reference_dto(row: ToolReferenceRow) -> ToolReferenceDto {
    ToolReferenceDto {
        kind: row.kind,
        ref_id: row.ref_id,
        name: row.name,
    }
}

fn write_ceiling(
    repositories: &Repositories<'_>,
    subject: &str,
    tier: ToolTier,
    granted_by: &str,
    granted_at: Timestamp,
) -> Result<(), DataError> {
    let existing = repositories
        .permissions
        .list(subject)?
        .into_iter()
        .find(|row| row.scope == GLOBAL_SCOPE);
    repositories.permissions.grant(&ToolPermissionRow {
        id: existing.map_or_else(id_text, |row| row.id),
        subject: subject.to_owned(),
        scope: GLOBAL_SCOPE.to_owned(),
        tier,
        granted_at,
        granted_by: granted_by.to_owned(),
    })?;
    Ok(())
}

fn updated(row: Option<ToolRow>) -> Result<ToolRow, DataError> {
    row.ok_or_else(|| DataError::App(not_found()))
}

impl ToolRegistry {
    pub fn new(options: ToolRegistryOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                database: options.database,
                settings: options.settings,
                logger: options.logger,
                clock: options.clock,
                defaults: Mutex::new(ToolDefaultsDto::default()),
            }),
        }
    }

    fn now(&self) -> Timestamp {
        self.inner.clock.now()
    }

    fn remembered(&self) -> ToolDefaultsDto {
        *self
            .inner
            .defaults
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn defaults(&self) -> ToolDefaultsDto {
        let stored = self
            .inner
            .settings
            .as_ref()
            .and_then(|settings| settings.get(TOOL_DEFAULTS_KEY).ok().flatten());
        stored
            .and_then(|setting| serde_json::from_value::<ToolDefaultsDto>(setting.value).ok())
            .unwrap_or_else(|| self.remembered())
    }

    pub fn set_defaults(&self, next: ToolDefaultsDto) -> Result<ToolDefaultsDto, AppError> {
        let value = serde_json::to_value(next)
            .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        if let Some(settings) = &self.inner.settings {
            settings.set(TOOL_DEFAULTS_KEY, &value)?;
        }
        *self
            .inner
            .defaults
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = next;
        self.inner.logger.log(
            LogLevel::Info,
            "tools",
            "Changed the default tiers for new tools",
            &[("defaults", value)],
        );
        Ok(next)
    }

    pub fn register(&self, descriptor: &ToolDescriptor) -> Result<ToolDto, AppError> {
        let descriptor = descriptor.normalized()?;
        let now = self.now();
        let preference = self.defaults().for_source(descriptor.source);
        let row = self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let Some(existing) = repositories.tools.find_by_key(&descriptor.key)? else {
                let tier = descriptor.tier.unwrap_or_else(|| {
                    default_tool_tier(descriptor.read_only, descriptor.destructive, preference)
                });
                let created = repositories.tools.insert(&ToolRow {
                    id: id_text(),
                    key: descriptor.key.clone(),
                    source: descriptor.source,
                    source_ref: descriptor.source_ref.clone(),
                    display_name: descriptor
                        .display_name
                        .clone()
                        .unwrap_or_else(|| descriptor.key.clone()),
                    description: descriptor.description.clone(),
                    input_schema: descriptor.input_schema.clone(),
                    read_only: descriptor.read_only,
                    destructive: descriptor.destructive,
                    tier,
                    enabled: true,
                    available: true,
                    last_used_at: None,
                    call_count: 0,
                    registered_at: now,
                    updated_at: now,
                })?;
                write_ceiling(&repositories, &created.key, tier, "registry", now)?;
                return Ok(created);
            };
            let tier = if descriptor.destructive && !existing.destructive {
                descriptor.tier.unwrap_or_else(|| {
                    default_tool_tier(
                        descriptor.read_only,
                        descriptor.destructive,
                        ToolDefaultTier::Inferred,
                    )
                })
            } else {
                existing.tier
            };
            let saved = updated(
                repositories.tools.update(
                    &existing.id,
                    &ToolPatch {
                        source: Some(descriptor.source),
                        source_ref: Some(descriptor.source_ref.clone()),
                        display_name: Some(
                            descriptor
                                .display_name
                                .clone()
                                .unwrap_or_else(|| existing.display_name.clone()),
                        ),
                        description: Some(descriptor.description.clone()),
                        input_schema: Some(descriptor.input_schema.clone()),
                        read_only: Some(descriptor.read_only),
                        destructive: Some(descriptor.destructive),
                        tier: Some(tier),
                        available: Some(true),
                        updated_at: Some(now),
                        ..ToolPatch::default()
                    },
                )?,
            )?;
            write_ceiling(&repositories, &saved.key, tier, "registry", now)?;
            Ok(saved)
        })?;
        self.inner.logger.log(
            LogLevel::Info,
            "tools",
            "Registered a tool",
            &[
                ("key", json!(row.key)),
                ("source", json!(row.source)),
                ("tier", json!(row.tier)),
            ],
        );
        tool_dto(&row)
    }

    pub fn deregister(&self, key: &str) -> Result<Option<ToolDto>, AppError> {
        check_tool_key("key", key)?;
        let now = self.now();
        let row = self.inner.database.with_connection(|connection| {
            let tools = Repositories::new(connection).tools;
            let Some(existing) = tools.find_by_key(js_trim(key))? else {
                return Ok(None);
            };
            Ok(tools.update(
                &existing.id,
                &ToolPatch {
                    available: Some(false),
                    updated_at: Some(now),
                    ..ToolPatch::default()
                },
            )?)
        })?;
        row.as_ref().map(tool_dto).transpose()
    }

    pub fn deregister_source(
        &self,
        source: ToolSource,
        source_ref: Option<&str>,
    ) -> Result<Vec<ToolDto>, AppError> {
        let now = self.now();
        let rows = self.inner.database.with_connection(|connection| {
            let tools = Repositories::new(connection).tools;
            let mut withdrawn = Vec::new();
            for row in tools.list_by_source(source, source_ref)? {
                withdrawn.push(updated(tools.update(
                    &row.id,
                    &ToolPatch {
                        available: Some(false),
                        updated_at: Some(now),
                        ..ToolPatch::default()
                    },
                )?)?);
            }
            Ok(withdrawn)
        })?;
        rows.iter().map(tool_dto).collect()
    }

    pub fn list(&self, filter: &ToolListFilter) -> Result<Vec<ToolDto>, AppError> {
        filter.validate()?;
        let needle = filter.query().map(str::to_lowercase);
        let include_unavailable = filter.include_unavailable();
        let rows = self
            .inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).tools.list()?))?;
        rows.iter()
            .filter(|row| include_unavailable || row.available)
            .filter(|row| filter.source.is_none_or(|source| row.source == source))
            .filter(|row| filter.tier.is_none_or(|tier| row.tier == tier))
            .filter(|row| {
                needle.as_deref().is_none_or(|needle| {
                    row.key.to_lowercase().contains(needle)
                        || row.display_name.to_lowercase().contains(needle)
                        || row.description.to_lowercase().contains(needle)
                })
            })
            .map(tool_dto)
            .collect()
    }

    fn require(&self, id: &str) -> Result<ToolRow, AppError> {
        if !is_wire_uuid(id) {
            return Err(invalid("id", "uuid"));
        }
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).tools.find_by_id(id)?)
            })?
            .ok_or_else(not_found)
    }

    fn reference_rows(&self, tool_id: &str) -> Result<Vec<ToolReferenceDto>, AppError> {
        Ok(self
            .inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).tools.references(tool_id)?)
            })?
            .into_iter()
            .map(reference_dto)
            .collect())
    }

    pub fn get(&self, id: &str) -> Result<ToolDetailDto, AppError> {
        let row = self.require(id)?;
        Ok(ToolDetailDto {
            tool: tool_dto(&row)?,
            references: self.reference_rows(&row.id)?,
        })
    }

    pub fn find_by_key(&self, key: &str) -> Result<Option<ToolDto>, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).tools.find_by_key(key)?)
            })?
            .as_ref()
            .map(tool_dto)
            .transpose()
    }

    pub fn usage(&self, id: &str) -> Result<ToolUsageDto, AppError> {
        let row = self.require(id)?;
        let dto = tool_dto(&row)?;
        Ok(ToolUsageDto {
            id: dto.id,
            key: dto.key,
            call_count: dto.call_count,
            last_used_at: dto.last_used_at,
            references: self.reference_rows(&row.id)?,
        })
    }

    pub fn set_tier(&self, id: &str, tier: ToolTier, actor: &str) -> Result<ToolDto, AppError> {
        let row = self.require(id)?;
        let now = self.now();
        let saved = self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let saved = updated(repositories.tools.update(
                &row.id,
                &ToolPatch {
                    tier: Some(tier),
                    updated_at: Some(now),
                    ..ToolPatch::default()
                },
            )?)?;
            write_ceiling(&repositories, &saved.key, tier, actor, now)?;
            Ok(saved)
        })?;
        tool_dto(&saved)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<ToolDto, AppError> {
        let row = self.require(id)?;
        let now = self.now();
        let saved = self.inner.database.with_connection(|connection| {
            updated(Repositories::new(connection).tools.update(
                &row.id,
                &ToolPatch {
                    enabled: Some(enabled),
                    updated_at: Some(now),
                    ..ToolPatch::default()
                },
            )?)
        })?;
        tool_dto(&saved)
    }

    pub fn ceiling(&self, key: &str) -> Result<Option<ToolCeiling>, AppError> {
        Ok(self
            .inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).tools.find_by_key(key)?)
            })?
            .map(|row| ToolCeiling {
                tier: row.tier,
                enabled: row.enabled,
                available: row.available,
            }))
    }

    pub fn record_use(&self, key: &str, used_at: Timestamp) -> Result<(), AppError> {
        self.inner.database.with_connection(|connection| {
            let tools = Repositories::new(connection).tools;
            if let Some(row) = tools.find_by_key(key)? {
                tools.record_use(&row.id, used_at)?;
            }
            Ok(())
        })
    }

    pub fn record_references(
        &self,
        subject: &ToolReferenceSubject,
        keys: &[&str],
    ) -> Result<(), AppError> {
        let ref_id = js_trim(&subject.ref_id).to_owned();
        let name = js_trim(&subject.name).to_owned();
        check::non_empty("refId", &ref_id)?;
        check::max_len("refId", &ref_id, 128)?;
        check::non_empty("name", &name)?;
        check::max_len("name", &name, 200)?;
        self.inner.database.transaction(|transaction| {
            let tools = Repositories::new(transaction).tools;
            let mut rows = Vec::new();
            for key in keys {
                if let Some(row) = tools.find_by_key(key)? {
                    rows.push(row);
                }
            }
            let kept: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
            tools.clear_references(subject.kind, &ref_id, &kept)?;
            for row in rows {
                tools.link_reference(&ToolReferenceRow {
                    id: id_text(),
                    tool_id: row.id,
                    kind: subject.kind,
                    ref_id: ref_id.clone(),
                    name: name.clone(),
                })?;
            }
            Ok(())
        })
    }

    pub fn references(&self, id: &str) -> Result<Vec<ToolReferenceDto>, AppError> {
        let row = self.require(id)?;
        self.reference_rows(&row.id)
    }
}
