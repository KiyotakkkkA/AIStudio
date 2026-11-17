use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zvs_contract::{AppError, AppErrorCode, TransferCategory};

use crate::data::DataError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
}

pub struct Table {
    pub name: &'static str,
    pub category: TransferCategory,
    pub columns: &'static str,
    pub primary: bool,
}

use TransferCategory::{Connections, Conversations, Memory, Scenarios, Settings, Skills, Stores};

pub const TABLES: &[Table] = &[
    Table {
        name: "provider",
        category: Connections,
        columns: "id,kind,adapter,auth_mode,name,base_url,capabilities,cap_text,cap_embedding,cap_image,settings,default_model_id,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "model",
        category: Connections,
        columns: "id,provider_id,external_id,display_name,family,context_window,max_output,size_bytes,capabilities,available,discovered_at,is_free,no_training",
        primary: false,
    },
    Table {
        name: "mcp_server",
        category: Connections,
        columns: "id,name,namespace,transport,command,args,env,url,headers,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "integration_account",
        category: Connections,
        columns: "id,kind,name,config,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "vector_store",
        category: Stores,
        columns: "id,name,description,backend,embedding_provider_id,embedding_model_id,dimension,metric,chunk_size,chunk_overlap,rerank,ocr,query_prefix,document_prefix,search_mode,chat_min_score,schema_version,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "vector_document",
        category: Stores,
        columns: "id,store_id,source_path,content_hash,chunk_count,bytes,indexed_at",
        primary: false,
    },
    Table {
        name: "conversation",
        category: Conversations,
        columns: "id,title,provider_id,model_id,settings,system_prompt,attached_store_ids,use_relations,retrieval_mode,profile,pinned,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "message",
        category: Conversations,
        columns: "id,conversation_id,role,content,reasoning,sources,images,selections,commands,plugins,offered_tools,rounds,tokens_in,tokens_out,tokens_cached,tokens_reasoning,usage_estimated,duration_ms,partial,pinned,event,created_at,recap",
        primary: false,
    },
    Table {
        name: "message_tool_call",
        category: Conversations,
        columns: "message_id,idx,call",
        primary: false,
    },
    Table {
        name: "chat_compaction",
        category: Conversations,
        columns: "id,conversation_id,boundary_message_id,summary,messages,tokens_before,tokens_after,provider_id,model_id,trigger,created_at",
        primary: false,
    },
    Table {
        name: "scenario",
        category: Scenarios,
        columns: "id,name,tags,overlap,current_version_id,trigger,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "scenario_version",
        category: Scenarios,
        columns: "id,scenario_id,version,graph,note,summary,created_at",
        primary: false,
    },
    Table {
        name: "scenario_draft",
        category: Scenarios,
        columns: "scenario_id,graph,saved_at",
        primary: false,
    },
    Table {
        name: "skill",
        category: Skills,
        columns: "id,key,name,description,source,status,issues,file_count,content_hash,path,version,author,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "memory",
        category: Memory,
        columns: "id,scope,conversation_id,content,source,created_at,updated_at",
        primary: true,
    },
    Table {
        name: "setting",
        category: Settings,
        columns: "key,value,updated_at",
        primary: true,
    },
];

pub fn table(kind: &str) -> Option<&'static Table> {
    TABLES.iter().find(|table| table.name == kind)
}

pub fn portable_setting(key: &str) -> bool {
    matches!(
        key,
        "ui.theme"
            | "ui.palette.dark"
            | "ui.palette.light"
            | "ui.rail.themepicker"
            | "ui.font.family"
            | "ui.font.size"
            | "ui.fx.preset"
            | "ui.fx.motion"
            | "ui.fx.effects"
            | "profile.style"
            | "profile.style.custom"
            | "profile.about"
            | "chat.toolcalls"
            | "chat.toolcalls.unlimited"
            | "notifications.enabled"
            | "notifications.approval"
            | "notifications.turn"
            | "notifications.download"
    )
}

pub fn invalid_package() -> AppError {
    AppError::new(
        AppErrorCode::UnsupportedFormat,
        "Файл данных повреждён или имеет неподдерживаемый формат",
    )
}

pub fn text<'a>(record: &'a Record, key: &str) -> Result<&'a str, AppError> {
    record
        .fields
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(invalid_package)
}

pub fn set(record: &mut Record, key: &str, value: impl Into<Value>) {
    record.fields.insert(key.to_owned(), value.into());
}

pub fn is_json_field(kind: &str, field: &str) -> bool {
    match field {
        "summary" => kind == "scenario_version",
        "trigger" => kind == "scenario",
        _ => matches!(
            field,
            "settings"
                | "capabilities"
                | "env"
                | "headers"
                | "args"
                | "config"
                | "tags"
                | "rerank"
                | "ocr"
                | "attached_store_ids"
                | "sources"
                | "images"
                | "selections"
                | "commands"
                | "plugins"
                | "offered_tools"
                | "rounds"
                | "event"
                | "graph"
                | "issues"
                | "call"
                | "value"
        ),
    }
}

pub fn validate_fields(record: &Record) -> Result<(), AppError> {
    use zvs_contract::validate::check;
    for (key, value) in &record.fields {
        if record.kind == "skill" && key == "version" {
            continue;
        }
        if value.is_null()
            && matches!(
                key.as_str(),
                "context_window" | "max_output" | "size_bytes" | "is_free" | "no_training"
            )
        {
            continue;
        }
        if matches!(
            key.as_str(),
            "idx"
                | "created_at"
                | "updated_at"
                | "discovered_at"
                | "indexed_at"
                | "saved_at"
                | "tokens_in"
                | "tokens_out"
                | "tokens_cached"
                | "tokens_reasoning"
                | "duration_ms"
                | "size_bytes"
                | "max_output"
                | "context_window"
                | "dimension"
                | "schema_version"
                | "chunk_size"
                | "chunk_overlap"
                | "version"
                | "file_count"
                | "bytes"
                | "chunk_count"
                | "messages"
                | "tokens_before"
                | "tokens_after"
        ) {
            check::safe_integer(key, value.as_u64().ok_or_else(invalid_package)?)
                .map_err(|_| invalid_package())?;
        }
        if matches!(
            key.as_str(),
            "cap_text"
                | "cap_embedding"
                | "cap_image"
                | "available"
                | "is_free"
                | "no_training"
                | "pinned"
                | "use_relations"
                | "partial"
                | "usage_estimated"
        ) && !matches!(value.as_u64(), Some(0 | 1))
        {
            return Err(invalid_package());
        }
        if is_json_field(&record.kind, key) && !value.is_null() {
            let _: Value = serde_json::from_str(value.as_str().ok_or_else(invalid_package)?)
                .map_err(|_| invalid_package())?;
        }
    }
    if let Some(name) = record.fields.get("name") {
        check::trimmed_name("name", name.as_str().ok_or_else(invalid_package)?, 120)
            .map_err(|_| invalid_package())?;
    }
    if record.kind == "memory" {
        check::trimmed_name(
            "content",
            text(record, "content")?,
            zvs_contract::memory::MEMORY_CONTENT_MAX,
        )
        .map_err(|_| invalid_package())?;
    }
    if record.kind == "skill" && !zvs_contract::is_skill_key(text(record, "key")?) {
        return Err(invalid_package());
    }
    for key in ["dimension", "chunk_size"] {
        if record
            .fields
            .get(key)
            .is_some_and(|value| !value.is_null() && value.as_u64() == Some(0))
        {
            return Err(invalid_package());
        }
    }
    Ok(())
}

fn sanitized(record: &mut Record) -> Result<(), AppError> {
    match record.kind.as_str() {
        "provider" => {
            let settings: Value =
                serde_json::from_str(text(record, "settings")?).map_err(|_| invalid_package())?;
            let mut safe = serde_json::Map::new();
            for key in [
                "openaiMode",
                "temperature",
                "topK",
                "topP",
                "maxOutputTokens",
                "timeoutSeconds",
                "selectedModelIds",
            ] {
                if let Some(value) = settings.get(key) {
                    safe.insert(key.to_owned(), value.clone());
                }
            }
            set(record, "settings", Value::Object(safe).to_string());
        }
        "mcp_server" => {
            for key in ["env", "headers"] {
                let mut values: BTreeMap<String, Value> =
                    serde_json::from_str(text(record, key)?).map_err(|_| invalid_package())?;
                for value in values.values_mut() {
                    if value.get("secretId").is_some() {
                        *value = serde_json::json!({"kind":"literal","value":""});
                    }
                }
                set(
                    record,
                    key,
                    serde_json::to_string(&values).map_err(|_| invalid_package())?,
                );
            }
        }
        "integration_account" => {
            let mut values: BTreeMap<String, Value> =
                serde_json::from_str(text(record, "config")?).map_err(|_| invalid_package())?;
            for value in values.values_mut() {
                if value.get("secretId").is_some() {
                    *value = Value::String(String::new());
                }
            }
            set(
                record,
                "config",
                serde_json::to_string(&values).map_err(|_| invalid_package())?,
            );
        }
        _ => {}
    }
    Ok(())
}

pub struct TransferRepository<'a> {
    connection: &'a Connection,
}

impl<'a> TransferRepository<'a> {
    pub const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn export(&self, categories: &[TransferCategory]) -> Result<Vec<Record>, DataError> {
        let mut records = Vec::new();
        for table in TABLES
            .iter()
            .filter(|table| categories.contains(&table.category))
        {
            let columns: Vec<_> = table.columns.split(',').collect();
            let mut query = self
                .connection
                .prepare(&format!("SELECT {} FROM {}", table.columns, table.name))?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                let mut fields = BTreeMap::new();
                for (index, column) in columns.iter().enumerate() {
                    let value = match row.get::<_, SqlValue>(index)? {
                        SqlValue::Null => Value::Null,
                        SqlValue::Integer(value) => value.into(),
                        SqlValue::Real(value) => value.into(),
                        SqlValue::Text(value) => value.into(),
                        SqlValue::Blob(_) => return Err(invalid_package().into()),
                    };
                    fields.insert((*column).to_owned(), value);
                }
                let mut record = Record {
                    kind: table.name.to_owned(),
                    fields,
                };
                if table.name == "setting" && !portable_setting(text(&record, "key")?) {
                    continue;
                }
                if table.name == "memory"
                    && !categories.contains(&Conversations)
                    && text(&record, "scope")? != "account"
                {
                    continue;
                }
                sanitized(&mut record)?;
                records.push(record);
            }
        }
        Ok(records)
    }

    pub fn check_idle(&self) -> Result<(), DataError> {
        let busy: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM run WHERE status IN ('queued','running','blocked'))",
            [],
            |row| row.get(0),
        )?;
        if busy {
            return Err(
                AppError::new(AppErrorCode::Conflict, "Дождитесь завершения задач")
                    .with_detail("reason", "busy")
                    .into(),
            );
        }
        Ok(())
    }

    pub fn blocked_categories(
        &self,
        records: &[Record],
    ) -> Result<Vec<TransferCategory>, DataError> {
        let providers = self.names(table("provider").ok_or_else(invalid_package)?, "id")?;
        let packaged_providers: HashSet<_> = records
            .iter()
            .filter(|record| record.kind == "provider")
            .map(|record| text(record, "id"))
            .collect::<Result<_, _>>()?;
        let conversations: HashSet<_> = records
            .iter()
            .filter(|record| record.kind == "conversation")
            .map(|record| text(record, "id"))
            .collect::<Result<_, _>>()?;
        let mut blocked = Vec::new();
        for record in records {
            let missing = match record.kind.as_str() {
                "vector_store" => {
                    let id = text(record, "embedding_provider_id")?;
                    !providers.contains(id) && !packaged_providers.contains(id)
                }
                "memory" if text(record, "scope")? == "conversation" => {
                    !conversations.contains(text(record, "conversation_id")?)
                }
                _ => false,
            };
            let category = table(&record.kind).ok_or_else(invalid_package)?.category;
            if missing && !blocked.contains(&category) {
                blocked.push(category);
            }
        }
        Ok(blocked)
    }

    pub fn imported(&self, hash: &str, category: TransferCategory) -> Result<bool, DataError> {
        self.flag(&format!("transfer.imported.{hash}.{}", category.as_str()))
    }

    pub fn imported_ids(&self, hash: &str) -> Result<HashMap<String, String>, DataError> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM setting WHERE key=?1",
                [format!("transfer.ids.{hash}")],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| DataError::App(invalid_package()))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn save_imported_ids(
        &self,
        hash: &str,
        ids: &HashMap<String, String>,
        now: i64,
    ) -> Result<(), DataError> {
        let value = serde_json::to_string(ids).map_err(|_| invalid_package())?;
        self.connection.execute("INSERT INTO setting (key,value,updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at", rusqlite::params![format!("transfer.ids.{hash}"),value,now])?;
        Ok(())
    }

    pub fn flag(&self, key: &str) -> Result<bool, DataError> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM setting WHERE key = ?1)",
            [key],
            |row| row.get(0),
        )?)
    }

    pub fn mark(&self, key: &str, now: i64) -> Result<(), DataError> {
        self.connection.execute(
            "INSERT INTO setting (key,value,updated_at) VALUES (?1,'true',?2)",
            rusqlite::params![key, now],
        )?;
        Ok(())
    }

    pub fn names(&self, table: &Table, field: &str) -> Result<HashSet<String>, DataError> {
        if !table.columns.split(',').any(|column| column == field) {
            return Err(invalid_package().into());
        }
        Ok(self
            .connection
            .prepare(&format!("SELECT {field} FROM {}", table.name))?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn insert(&self, record: &Record) -> Result<(), DataError> {
        let spec = table(&record.kind).ok_or_else(invalid_package)?;
        let columns: Vec<_> = spec.columns.split(',').collect();
        if record.fields.len() != columns.len()
            || columns.iter().any(|key| !record.fields.contains_key(*key))
        {
            return Err(invalid_package().into());
        }
        let mut values = Vec::new();
        for key in &columns {
            values.push(match &record.fields[*key] {
                Value::Null => SqlValue::Null,
                Value::String(value) => SqlValue::Text(value.clone()),
                Value::Number(value) => match value.as_i64() {
                    Some(value) => SqlValue::Integer(value),
                    None => SqlValue::Real(value.as_f64().ok_or_else(invalid_package)?),
                },
                _ => return Err(invalid_package().into()),
            });
        }
        let placeholders = vec!["?"; columns.len()].join(",");
        let extra = match spec.name {
            "provider" | "mcp_server" | "integration_account" | "skill" => ",enabled",
            "scenario" => ",active",
            _ => "",
        };
        let extra_value = if extra.is_empty() { "" } else { ",0" };
        let suffix = if spec.name == "setting" {
            " ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at"
        } else {
            ""
        };
        self.connection.execute(
            &format!(
                "INSERT INTO {} ({}{extra}) VALUES ({placeholders}{extra_value}){suffix}",
                spec.name, spec.columns
            ),
            params_from_iter(values),
        )?;
        Ok(())
    }

    pub fn insert_source(&self, owner: &str, path: &str, now: i64) -> Result<(), DataError> {
        self.connection.execute("INSERT INTO vector_source (id,store_id,kind,path,include,exclude,recursive,created_at) VALUES (?1,?2,'file',?3,'[]','[]',0,?4)", rusqlite::params![uuid::Uuid::new_v4().to_string(), owner, path, now])?;
        Ok(())
    }

    pub fn check_memory_limits(&self) -> Result<(), DataError> {
        let mut query = self
            .connection
            .prepare("SELECT scope, count(*) FROM memory GROUP BY scope, conversation_id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let scope: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            let limit = if scope == "account" {
                zvs_contract::MEMORY_ACCOUNT_MAX
            } else {
                zvs_contract::MEMORY_CONVERSATION_MAX
            };
            if count > limit as i64 {
                return Err(
                    AppError::new(AppErrorCode::Conflict, "Превышен лимит записей памяти")
                        .with_detail("reason", "memoryLimit")
                        .into(),
                );
            }
        }
        Ok(())
    }

    pub fn validate_record(&self, record: &Record) -> Result<(), DataError> {
        use crate::data::schema::*;
        use zvs_contract::Validate;
        macro_rules! row {
            ($row:ident) => {
                self.connection
                    .query_row(
                        &format!("SELECT {} FROM {} WHERE id=?1", $row::COLUMNS, record.kind),
                        [text(record, "id")?],
                        $row::from_row,
                    )
                    .map_err(|_| invalid_package())?
            };
        }
        match record.kind.as_str() {
            "provider" => {
                let row = row!(ProviderRow);
                if zvs_contract::AdapterFamily::parse(&row.adapter).is_none()
                    || zvs_contract::AuthMode::parse(&row.auth_mode).is_none()
                {
                    return Err(invalid_package().into());
                }
                zvs_contract::validate::parse::<zvs_contract::ProviderSettingsDto>(Value::Object(
                    row.settings.0,
                ))
                .map_err(|_| invalid_package())?;
            }
            "model" => {
                let _ = row!(ModelRow);
            }
            "vector_document" => {
                let _ = row!(VectorDocumentRow);
            }
            "vector_store" => {
                let row = row!(VectorStoreRow);
                zvs_contract::validate::parse::<zvs_contract::CreateVectorStoreInput>(serde_json::json!({
                    "name": row.name, "embeddingProviderId": row.embedding_provider_id,
                    "embeddingModelId": row.embedding_model_id, "backend": row.backend,
                    "dimension": row.dimension, "metric": row.metric, "chunkSize": row.chunk_size,
                    "chunkOverlap": row.chunk_overlap, "rerank": row.rerank, "ocr": row.ocr,
                    "searchMode": row.search_mode, "chatMinScore": row.chat_min_score,
                })).map_err(|_| invalid_package())?;
            }
            "conversation" => {
                let row = row!(ConversationRow);
                zvs_contract::validate::parse::<zvs_contract::CreateConversationInput>(serde_json::json!({
                    "title": row.title, "providerId": row.provider_id, "modelId": row.model_id,
                    "settings": row.settings, "systemPrompt": row.system_prompt,
                    "attachedStoreIds": row.attached_store_ids, "retrievalMode": row.retrieval_mode,
                })).map_err(|_| invalid_package())?;
            }
            "message" => {
                let row = row!(MessageRow);
                row.images.validate()?;
                row.sources.validate()?;
                row.selections.validate()?;
                row.tool_calls.validate()?;
                row.rounds.validate()?;
                row.offered_tools.validate()?;
                row.event.validate()?;
            }
            "chat_compaction" => {
                let _ = row!(CompactionRow);
            }
            "mcp_server" => {
                let row = row!(McpServerRow);
                zvs_contract::mcp::check_env("env", &row.env)?;
                zvs_contract::mcp::check_headers("headers", &row.headers)?;
                zvs_contract::mcp::check_namespace("namespace", &row.namespace)?;
            }
            "integration_account" => {
                let row = row!(IntegrationAccountRow);
                for value in row.config.values() {
                    value.validate()?;
                }
            }
            "scenario" => {
                let row = row!(ScenarioRow);
                row.trigger.validate()?;
            }
            "scenario_version" | "scenario_draft" => {
                let graph: Value =
                    serde_json::from_str(text(record, "graph")?).map_err(|_| invalid_package())?;
                zvs_contract::validate::parse::<zvs_contract::ScenarioGraphDto>(graph)
                    .map_err(|_| invalid_package())?;
                if record.kind == "scenario_version" {
                    let row = row!(ScenarioVersionRow);
                    row.summary.validate()?;
                }
            }
            "skill" => {
                let row = row!(SkillRow);
                row.issues.validate()?;
            }
            "memory" => {
                let _ = row!(MemoryRow);
            }
            "setting" => {
                let _: Value =
                    serde_json::from_str(text(record, "value")?).map_err(|_| invalid_package())?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn id_map(records: &[Record]) -> Result<HashMap<String, String>, AppError> {
    let mut ids = HashMap::new();
    for record in records {
        if let Some(value) = record.fields.get("id") {
            let id = value.as_str().ok_or_else(invalid_package)?;
            if uuid::Uuid::parse_str(id).is_err()
                || ids
                    .insert(id.to_owned(), uuid::Uuid::new_v4().to_string())
                    .is_some()
            {
                return Err(invalid_package());
            }
        }
    }
    Ok(ids)
}
