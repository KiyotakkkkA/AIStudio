use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zvs_contract::{AppError, AppErrorCode, TransferCategory, TransferImportDto};

use super::archive::{Package, local_path};
use super::{failure, io_error};
use crate::data::repositories::data_transfer::{
    Record, TABLES, TransferRepository, id_map, invalid_package, is_json_field, set, table, text,
};
use crate::data::{DataError, Database};
use crate::platform::paths::StudioPaths;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    batch: String,
    conversations: Vec<String>,
    skills: Vec<String>,
}

impl Journal {
    fn targets(&self, paths: &StudioPaths) -> Result<Vec<PathBuf>, AppError> {
        for id in std::iter::once(&self.batch)
            .chain(&self.conversations)
            .chain(&self.skills)
        {
            if uuid::Uuid::parse_str(id).is_err() {
                return Err(invalid_package());
            }
        }
        let mut targets = vec![paths.imports_dir.join(&self.batch)];
        targets.extend(
            self.conversations
                .iter()
                .map(|id| paths.conversations_dir.join(id)),
        );
        targets.extend(
            self.skills
                .iter()
                .map(|id| paths.skills_dir.join(format!("import-{id}"))),
        );
        Ok(targets)
    }

    fn cleanup(&self, paths: &StudioPaths) -> Result<(), AppError> {
        for target in self.targets(paths)? {
            match fs::symlink_metadata(&target) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Err(invalid_package()),
                Ok(_) => fs::remove_dir_all(&target).map_err(io_error)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        Ok(())
    }
}

pub fn recover(paths: &StudioPaths, database: &Database) -> Result<(), AppError> {
    let directory = paths.transfer_journals_dir.clone();
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(error)),
    };
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        if !entry.file_type().map_err(io_error)?.is_file()
            || entry.metadata().map_err(io_error)?.len() > 1024 * 1024
        {
            return Err(invalid_package());
        }
        let journal: Journal = serde_json::from_slice(&fs::read(entry.path()).map_err(io_error)?)
            .map_err(|_| invalid_package())?;
        journal.targets(paths)?;
        let committed = database.with_connection(|connection| {
            TransferRepository::new(connection)
                .flag(&format!("transfer.committed.{}", journal.batch))
        })?;
        if !committed {
            journal.cleanup(paths)?;
        }
        fs::remove_file(entry.path()).map_err(io_error)?;
    }
    Ok(())
}

fn rewrite_json(
    value: &mut Value,
    ids: &HashMap<String, String>,
    files: &HashMap<String, String>,
    key: &str,
) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                rewrite_json(value, ids, files, key);
            }
        }
        Value::Array(items) => {
            for value in items {
                rewrite_json(value, ids, files, key);
            }
        }
        Value::String(value) => {
            if matches!(
                key,
                "providerId"
                    | "conversationId"
                    | "messageId"
                    | "documentId"
                    | "storeId"
                    | "vectorStoreId"
                    | "serverId"
                    | "integrationId"
                    | "storeIds"
                    | "attachedStoreIds"
                    | "accountId"
                    | "scenarioId"
                    | "versionId"
                    | "boundaryMessageId"
            ) && let Some(next) = ids.get(value)
            {
                *value = next.clone();
            }
            if matches!(
                key,
                "path" | "sourcePath" | "filePath" | "imagePath" | "url"
            ) && let Some(next) = files.get(value)
            {
                *value = next.clone();
            }
        }
        _ => {}
    }
}

fn unique(value: &str, taken: &mut HashSet<String>, limit: usize, separator: &str) -> String {
    if taken.insert(value.to_owned()) {
        return value.to_owned();
    }
    let mut used = 0;
    let stem: String = value
        .chars()
        .take_while(|character| {
            used += character.len_utf16();
            used <= limit.saturating_sub(16)
        })
        .collect();
    for number in 2_u64.. {
        let candidate = format!("{stem}{separator}{number}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}

fn prepare(
    records: &mut [Record],
    repository: &TransferRepository<'_>,
    ids: &HashMap<String, String>,
    paths: &StudioPaths,
    files: &HashMap<String, String>,
) -> Result<(), DataError> {
    let mut names = HashMap::new();
    let mut namespaces = HashMap::new();
    let mut stores = repository.names(table("vector_store").ok_or_else(invalid_package)?, "id")?;
    for record in records
        .iter()
        .filter(|record| record.kind == "vector_store")
    {
        stores.insert(ids[text(record, "id")?].clone());
    }
    let mapped_ids: HashSet<String> = ids.values().cloned().collect();
    for spec in TABLES {
        for field in ["name", "namespace", "key"] {
            if spec.columns.split(',').any(|column| column == field) && spec.name != "setting" {
                names.insert(
                    (spec.name.to_owned(), field.to_owned()),
                    repository.names(spec, field)?,
                );
            }
        }
    }
    for record in records.iter_mut() {
        for (field, value) in &mut record.fields {
            if matches!(
                field.as_str(),
                "id" | "provider_id"
                    | "default_model_id"
                    | "embedding_provider_id"
                    | "store_id"
                    | "scenario_id"
                    | "current_version_id"
                    | "conversation_id"
                    | "message_id"
                    | "boundary_message_id"
                    | "run_id"
                    | "subject_id"
            ) {
                if let Some(old) = value.as_str() {
                    if let Some(next) = ids.get(old) {
                        *value = next.clone().into();
                    } else if field == "provider_id"
                        && matches!(record.kind.as_str(), "conversation" | "chat_compaction")
                    {
                        if uuid::Uuid::parse_str(old).is_err() {
                            return Err(invalid_package().into());
                        }
                    } else if matches!(field.as_str(), "provider_id" | "embedding_provider_id") {
                        let found: bool = repository
                            .names(table("provider").ok_or_else(invalid_package)?, "id")?
                            .contains(old);
                        if !found {
                            return Err(failure(AppErrorCode::Conflict, "dependencies").into());
                        }
                    } else {
                        return Err(failure(AppErrorCode::Conflict, "dependencies").into());
                    }
                }
            } else if matches!(
                field.as_str(),
                "settings"
                    | "attached_store_ids"
                    | "sources"
                    | "images"
                    | "selections"
                    | "plugins"
                    | "rounds"
                    | "offered_tools"
                    | "call"
                    | "graph"
                    | "summary"
                    | "trigger"
                    | "rerank"
                    | "ocr"
                    | "event"
            ) && is_json_field(&record.kind, field)
                && !value.is_null()
            {
                let mut json: Value =
                    serde_json::from_str(value.as_str().ok_or_else(invalid_package)?)
                        .map_err(|_| invalid_package())?;
                let key = if field == "attached_store_ids" {
                    "attachedStoreIds"
                } else {
                    ""
                };
                rewrite_json(&mut json, ids, files, key);
                if field == "attached_store_ids"
                    && let Value::Array(items) = &mut json
                {
                    items.retain(|item| {
                        item.as_str()
                            .is_some_and(|id| stores.contains(id) && mapped_ids.contains(id))
                    });
                }
                *value = json.to_string().into();
            }
        }
        let kind = record.kind.clone();
        for field in ["name", "namespace", "key"] {
            if kind == "setting" {
                continue;
            }
            if let Some(taken) = names.get_mut(&(kind.clone(), field.to_owned())) {
                let old = text(record, field)?.to_owned();
                let next = unique(
                    &old,
                    taken,
                    if field == "namespace" {
                        48
                    } else if field == "key" {
                        63
                    } else {
                        120
                    },
                    if field == "namespace" { "" } else { "-" },
                );
                if kind == "mcp_server" && field == "namespace" {
                    namespaces.insert(old.clone(), next.clone());
                }
                set(record, field, next);
            }
        }
        match kind.as_str() {
            "vector_document" => {
                let old = text(record, "source_path")?;
                let restored = files.get(old).ok_or_else(invalid_package)?;
                set(
                    record,
                    "source_path",
                    paths
                        .stored_paths()
                        .decode(restored)
                        .to_string_lossy()
                        .into_owned(),
                );
                set(record, "content_hash", "dbox:pending");
                set(record, "chunk_count", 0);
                set(record, "indexed_at", 0);
            }
            "skill" => {
                let id = text(record, "id")?;
                let path = paths.skills_dir.join(format!("import-{id}"));
                set(record, "path", paths.stored_paths().encode(&path));
                set(record, "source", "local");
            }
            "provider" => {
                let settings: Value = serde_json::from_str(text(record, "settings")?)
                    .map_err(|_| invalid_package())?;
                if settings.as_object().is_none_or(|map| {
                    map.keys().any(|key| {
                        !matches!(
                            key.as_str(),
                            "openaiMode"
                                | "temperature"
                                | "topK"
                                | "topP"
                                | "maxOutputTokens"
                                | "timeoutSeconds"
                                | "selectedModelIds"
                        )
                    })
                }) {
                    return Err(invalid_package().into());
                }
            }
            "mcp_server" | "integration_account" => {
                let keys: &[&str] = if kind == "mcp_server" {
                    &["env", "headers"]
                } else {
                    &["config"]
                };
                for key in keys {
                    let values: BTreeMapString =
                        serde_json::from_str(text(record, key)?).map_err(|_| invalid_package())?;
                    if values.values().any(|value| value.get("secretId").is_some()) {
                        return Err(invalid_package().into());
                    }
                }
            }
            _ => {}
        }
    }
    for record in records {
        for field in ["graph", "call", "offered_tools", "rounds"] {
            if let Some(Value::String(encoded)) = record.fields.get_mut(field) {
                let mut value: Value =
                    serde_json::from_str(encoded).map_err(|_| invalid_package())?;
                rewrite_tools(&mut value, &namespaces, "");
                *encoded = value.to_string();
            }
        }
    }
    Ok(())
}

fn rewrite_tools(value: &mut Value, namespaces: &HashMap<String, String>, key: &str) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                rewrite_tools(value, namespaces, key);
            }
        }
        Value::Array(items) => {
            for value in items {
                rewrite_tools(value, namespaces, key);
            }
        }
        Value::String(value) if matches!(key, "type" | "key" | "toolKey" | "tools") => {
            if let Some((namespace, tail)) = value.split_once('.')
                && let Some(next) = namespaces.get(namespace)
            {
                *value = format!("{next}.{tail}");
            }
        }
        _ => {}
    }
}

type BTreeMapString = std::collections::BTreeMap<String, Value>;

pub fn restore(
    paths: &StudioPaths,
    database: &Database,
    now: i64,
    package: &Package,
    hash: &str,
    blobs: &Path,
    categories: &[TransferCategory],
) -> Result<TransferImportDto, AppError> {
    package
        .validate()
        .map_err(|error| error.with_detail("stage", "archive"))?;
    let mut ids = database.with_connection(|connection| {
        let repository = TransferRepository::new(connection);
        for category in categories {
            if repository.imported(hash, *category)? {
                return Err(failure(AppErrorCode::Conflict, "duplicate").into());
            }
        }
        for item in package.preview().categories {
            if categories.contains(&item.category) {
                for required in item.requires {
                    if !categories.contains(&required) && !repository.imported(hash, required)? {
                        return Err(failure(AppErrorCode::Conflict, "dependencies").into());
                    }
                }
            }
        }
        repository.imported_ids(hash)
    })?;
    if categories
        .iter()
        .any(|category| !package.categories.contains(category))
    {
        return Err(invalid_package());
    }
    super::require_space(
        &paths.data_dir,
        package
            .preview()
            .categories
            .iter()
            .filter(|item| categories.contains(&item.category))
            .map(|item| item.bytes)
            .sum::<u64>()
            + super::archive::MAX_JSON,
    )?;
    let mut records: Vec<_> = package
        .records
        .iter()
        .filter(|record| {
            table(&record.kind).is_some_and(|spec| categories.contains(&spec.category))
        })
        .cloned()
        .collect();
    ids.extend(id_map(&records)?);
    let batch = uuid::Uuid::new_v4().to_string();
    let mut journal = Journal {
        batch: batch.clone(),
        conversations: Vec::new(),
        skills: Vec::new(),
    };
    for record in &records {
        if record.kind == "conversation" {
            journal.conversations.push(ids[text(record, "id")?].clone());
        }
        if record.kind == "skill" {
            journal.skills.push(ids[text(record, "id")?].clone());
        }
    }
    let targets = journal.targets(paths)?;
    for target in &targets {
        if target.exists() {
            return Err(failure(AppErrorCode::Conflict, "destination"));
        }
    }
    let journal_dir = paths.transfer_journals_dir.clone();
    fs::create_dir_all(&journal_dir).map_err(io_error)?;
    let journal_path = journal_dir.join(format!("{batch}.json"));
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(&journal_path)
        .map_err(io_error)?;
    file.write_all(&serde_json::to_vec(&journal).map_err(io_error)?)
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    let outcome = database.transaction(|transaction| {
        let repository = TransferRepository::new(transaction);
        repository.check_idle()?;
        for category in categories {
            if repository.imported(hash, *category)? {
                return Err(failure(AppErrorCode::Conflict, "duplicate").into());
            }
        }
        for target in &targets {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::create_dir(target)?;
        }
        let mut file_paths = HashMap::new();
        let mut source_paths = Vec::new();
        for member in package
            .files
            .iter()
            .filter(|file| categories.contains(&file.category))
        {
            let owner = ids.get(&member.owner).ok_or_else(invalid_package)?;
            let root = match member.category {
                TransferCategory::Conversations => paths.conversations_dir.join(owner),
                TransferCategory::Skills => paths.skills_dir.join(format!("import-{owner}")),
                TransferCategory::Stores => {
                    paths.imports_dir.join(&batch).join("stores").join(owner)
                }
                _ => return Err(invalid_package().into()),
            };
            let destination = local_path(&root, &member.path)?;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(blobs.join(&member.hash), &destination)?;
            File::options().write(true).open(&destination)?.sync_all()?;
            let stored = paths.stored_paths().encode(&destination);
            file_paths.insert(member.stored_original.clone(), stored.clone());
            file_paths.insert(
                member.original.clone(),
                destination.to_string_lossy().into_owned(),
            );
            if member.category == TransferCategory::Stores {
                source_paths.push((owner.clone(), destination.to_string_lossy().into_owned()));
            }
        }
        for owner in &journal.conversations {
            let manifest = paths
                .conversations_dir
                .join(owner)
                .join("outputs/manifest.json");
            if manifest.exists() {
                let mut value: Value =
                    serde_json::from_slice(&fs::read(&manifest)?).map_err(|_| invalid_package())?;
                rewrite_json(&mut value, &ids, &file_paths, "");
                fs::write(&manifest, serde_json::to_vec(&value).map_err(io_error)?)?;
                File::options().write(true).open(&manifest)?.sync_all()?;
            }
        }
        prepare(&mut records, &repository, &ids, paths, &file_paths)?;
        let runs = records.iter().filter(|record| record.kind == "run");
        let other = records.iter().filter(|record| record.kind != "run");
        for record in runs.chain(other) {
            repository.insert(record)?;
        }
        for record in &records {
            repository
                .validate_record(record)
                .map_err(|error| match error {
                    DataError::App(error) => {
                        DataError::App(error.with_detail("recordKind", record.kind.clone()))
                    }
                    error => error,
                })?;
        }
        if categories.contains(&TransferCategory::Memory) {
            repository.check_memory_limits()?;
        }
        repository.save_imported_ids(hash, &ids, now)?;
        for (owner, path) in &source_paths {
            repository.insert_source(owner, path, now)?;
        }
        for category in categories {
            repository.mark(
                &format!("transfer.imported.{hash}.{}", category.as_str()),
                now,
            )?;
        }
        repository.mark(&format!("transfer.committed.{batch}"), now)?;
        Ok(())
    });
    if let Err(error) = outcome {
        journal.cleanup(paths)?;
        fs::remove_file(journal_path).map_err(io_error)?;
        return Err(error);
    }
    let _ = fs::remove_file(journal_path);
    Ok(TransferImportDto {
        categories: package
            .preview()
            .categories
            .into_iter()
            .filter(|item| categories.contains(&item.category))
            .collect(),
    })
}
