use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde_json::json;
use sha2::{Digest, Sha256};
use zvs_contract::{AppError, AppErrorCode};

use super::database::{DataError, Database};
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::paths::{backup_file_path, is_backup_file};

pub const BACKUPS_KEPT: usize = 3;

const JOURNAL: &str = "CREATE TABLE IF NOT EXISTS __migrations (\
    id INTEGER PRIMARY KEY NOT NULL, \
    name TEXT NOT NULL, \
    checksum TEXT NOT NULL, \
    applied_at INTEGER NOT NULL\
)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration<'a> {
    pub id: u32,
    pub name: &'a str,
    pub sql: &'a str,
}

impl Migration<'_> {
    pub fn normalised_sql(&self) -> String {
        self.sql.replace("\r\n", "\n")
    }

    pub fn checksum(&self) -> String {
        let digest = Sha256::digest(self.normalised_sql().as_bytes());
        digest
            .iter()
            .fold(String::with_capacity(64), |mut text, byte| {
                let _ = write!(text, "{byte:02x}");
                text
            })
    }
}

pub const MIGRATIONS: &[Migration<'static>] = &[
    Migration {
        id: 0,
        name: "0000_baseline",
        sql: include_str!("../../migrations/0000_baseline.sql"),
    },
    Migration {
        id: 1,
        name: "0001_graph_relation_seed",
        sql: include_str!("../../migrations/0001_graph_relation_seed.sql"),
    },
    Migration {
        id: 2,
        name: "0002_remove_deepseek_web",
        sql: include_str!("../../migrations/0002_remove_deepseek_web.sql"),
    },
    Migration {
        id: 3,
        name: "0003_mcp_oauth",
        sql: include_str!("../../migrations/0003_mcp_oauth.sql"),
    },
    Migration {
        id: 4,
        name: "0004_conversation_retrieval_mode",
        sql: include_str!("../../migrations/0004_conversation_retrieval_mode.sql"),
    },
    Migration {
        id: 5,
        name: "0005_mcp_server_instructions",
        sql: include_str!("../../migrations/0005_mcp_server_instructions.sql"),
    },
    Migration {
        id: 6,
        name: "0006_skill",
        sql: include_str!("../../migrations/0006_skill.sql"),
    },
    Migration {
        id: 7,
        name: "0007_folder_access",
        sql: include_str!("../../migrations/0007_folder_access.sql"),
    },
    Migration {
        id: 8,
        name: "0008_document",
        sql: include_str!("../../migrations/0008_document.sql"),
    },
    Migration {
        id: 9,
        name: "0009_document_office",
        sql: include_str!("../../migrations/0009_document_office.sql"),
    },
    Migration {
        id: 10,
        name: "0010_message_selections",
        sql: include_str!("../../migrations/0010_message_selections.sql"),
    },
    Migration {
        id: 11,
        name: "0011_document_office_native",
        sql: include_str!("../../migrations/0011_document_office_native.sql"),
    },
    Migration {
        id: 12,
        name: "0012_conversation_workspace_tools",
        sql: include_str!("../../migrations/0012_conversation_workspace_tools.sql"),
    },
    Migration {
        id: 13,
        name: "0013_scenario",
        sql: include_str!("../../migrations/0013_scenario.sql"),
    },
    Migration {
        id: 14,
        name: "0014_scenario_draft",
        sql: include_str!("../../migrations/0014_scenario_draft.sql"),
    },
    Migration {
        id: 15,
        name: "0015_scenario_overlap",
        sql: include_str!("../../migrations/0015_scenario_overlap.sql"),
    },
    Migration {
        id: 16,
        name: "0016_memory",
        sql: include_str!("../../migrations/0016_memory.sql"),
    },
    Migration {
        id: 17,
        name: "0017_conversation_profile",
        sql: include_str!("../../migrations/0017_conversation_profile.sql"),
    },
    Migration {
        id: 18,
        name: "0018_agentic_session",
        sql: include_str!("../../migrations/0018_agentic_session.sql"),
    },
    Migration {
        id: 19,
        name: "0019_skill_version",
        sql: include_str!("../../migrations/0019_skill_version.sql"),
    },
    Migration {
        id: 20,
        name: "0020_tool_surface",
        sql: include_str!("../../migrations/0020_tool_surface.sql"),
    },
    Migration {
        id: 21,
        name: "0021_skill_restrictions",
        sql: include_str!("../../migrations/0021_skill_restrictions.sql"),
    },
    Migration {
        id: 22,
        name: "0022_message_offered_tools",
        sql: include_str!("../../migrations/0022_message_offered_tools.sql"),
    },
    Migration {
        id: 23,
        name: "0023_append_only_history",
        sql: include_str!("../../migrations/0023_append_only_history.sql"),
    },
    Migration {
        id: 24,
        name: "0024_message_web_sources",
        sql: include_str!("../../migrations/0024_message_web_sources.sql"),
    },
    Migration {
        id: 25,
        name: "0025_browser_grant",
        sql: include_str!("../../migrations/0025_browser_grant.sql"),
    },
    Migration {
        id: 26,
        name: "0026_chat_commands",
        sql: include_str!("../../migrations/0026_chat_commands.sql"),
    },
    Migration {
        id: 27,
        name: "0027_pins",
        sql: include_str!("../../migrations/0027_pins.sql"),
    },
    Migration {
        id: 28,
        name: "0028_integration",
        sql: include_str!("../../migrations/0028_integration.sql"),
    },
    Migration {
        id: 29,
        name: "0029_message_rounds",
        sql: include_str!("../../migrations/0029_message_rounds.sql"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct MigrationFailed {
    pub message: String,
    pub backups_dir: PathBuf,
}

impl MigrationFailed {
    fn new(message: impl Into<String>, backups_dir: &Path) -> Self {
        Self {
            message: message.into(),
            backups_dir: backups_dir.to_path_buf(),
        }
    }
}

impl From<MigrationFailed> for AppError {
    fn from(failure: MigrationFailed) -> Self {
        Self::new(AppErrorCode::DbError, failure.message).with_detail(
            "backupsDir",
            failure.backups_dir.to_string_lossy().into_owned(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrateReport {
    pub applied: Vec<String>,
    pub backup: Option<PathBuf>,
}

pub struct MigrateOptions<'a> {
    pub backups_dir: &'a Path,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub keep: usize,
}

pub struct PrepareOptions<'a> {
    pub file: &'a Path,
    pub backups_dir: &'a Path,
    pub logger: SharedLogger,
    pub clock: SharedClock,
}

pub fn prepare_database(
    options: &PrepareOptions<'_>,
) -> Result<(Database, MigrateReport), MigrationFailed> {
    prepare_with(options, MIGRATIONS)
}

pub fn prepare_with(
    options: &PrepareOptions<'_>,
    migrations: &[Migration<'_>],
) -> Result<(Database, MigrateReport), MigrationFailed> {
    let database = Database::open(options.file, options.logger.clone()).map_err(|error| {
        options.logger.log(
            LogLevel::Error,
            "data",
            "Could not open the database",
            &[
                ("file", json!(options.file.to_string_lossy())),
                ("error", json!(error.to_string())),
            ],
        );
        MigrationFailed::new(error.to_string(), options.backups_dir)
    })?;
    let report = migrate(
        &database,
        migrations,
        &MigrateOptions {
            backups_dir: options.backups_dir,
            logger: options.logger.clone(),
            clock: options.clock.clone(),
            keep: BACKUPS_KEPT,
        },
    )?;
    Ok((database, report))
}

pub fn migrate(
    database: &Database,
    migrations: &[Migration<'_>],
    options: &MigrateOptions<'_>,
) -> Result<MigrateReport, MigrationFailed> {
    let mut connection = database.lock();
    run(&mut connection, migrations, options).map_err(|error| {
        let message = match error {
            DataError::App(error) => error.message,
            DataError::Sqlite(error) => error.to_string(),
            DataError::Io(error) => error.to_string(),
        };
        options.logger.log(
            LogLevel::Error,
            "data",
            "Migration failed",
            &[("error", json!(message))],
        );
        MigrationFailed::new(message, options.backups_dir)
    })
}

fn run(
    connection: &mut Connection,
    migrations: &[Migration<'_>],
    options: &MigrateOptions<'_>,
) -> Result<MigrateReport, DataError> {
    check_order(migrations)?;
    let tables = user_tables(connection)?;
    let applied = if tables.contains("__migrations") {
        journal(connection)?
    } else {
        BTreeMap::new()
    };
    for migration in migrations {
        if let Some(checksum) = applied.get(&migration.id)
            && *checksum != migration.checksum()
        {
            return Err(failure(format!(
                "Миграция {} изменена после применения: контрольная сумма не совпадает",
                migration.name
            )));
        }
    }

    let pending: Vec<&Migration<'_>> = migrations
        .iter()
        .filter(|migration| !applied.contains_key(&migration.id))
        .collect();
    if pending.is_empty() {
        options
            .logger
            .log(LogLevel::Info, "data", "Schema is up to date", &[]);
        return Ok(MigrateReport::default());
    }

    let backup = if tables.is_empty() {
        None
    } else {
        let backup = back_up(connection, options)?;
        prune_backups(options.backups_dir, options.keep)?;
        options.logger.log(
            LogLevel::Info,
            "data",
            "Backed up the database before migrating",
            &[("backup", json!(backup.to_string_lossy()))],
        );
        Some(backup)
    };

    let mut report = MigrateReport {
        applied: Vec::with_capacity(pending.len()),
        backup,
    };
    for migration in pending {
        let transaction = connection.transaction()?;
        transaction.execute_batch(JOURNAL)?;
        transaction.execute_batch(&migration.normalised_sql())?;
        transaction.execute(
            "INSERT INTO __migrations (id, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                migration.id,
                migration.name,
                migration.checksum(),
                options.clock.now()
            ],
        )?;
        transaction.commit()?;
        options.logger.log(
            LogLevel::Info,
            "data",
            "Applied migration",
            &[("migration", json!(migration.name))],
        );
        report.applied.push(migration.name.to_owned());
    }
    Ok(report)
}

fn failure(message: String) -> DataError {
    DataError::App(AppError::new(AppErrorCode::DbError, message))
}

fn check_order(migrations: &[Migration<'_>]) -> Result<(), DataError> {
    let ascending = migrations.windows(2).all(|pair| pair[0].id < pair[1].id);
    let names: BTreeSet<&str> = migrations.iter().map(|migration| migration.name).collect();
    if ascending && names.len() == migrations.len() {
        Ok(())
    } else {
        Err(failure(
            "Список миграций должен идти по возрастанию id без повторов имён".to_owned(),
        ))
    }
}

fn user_tables(connection: &Connection) -> Result<BTreeSet<String>, DataError> {
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(names)
}

fn journal(connection: &Connection) -> Result<BTreeMap<u32, String>, DataError> {
    let mut statement = connection.prepare("SELECT id, checksum FROM __migrations")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn back_up(connection: &Connection, options: &MigrateOptions<'_>) -> Result<PathBuf, DataError> {
    std::fs::create_dir_all(options.backups_dir)?;
    let target = backup_file_path(options.backups_dir, options.clock.now());
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    connection.execute("VACUUM INTO ?1", [target.to_string_lossy()])?;
    Ok(target)
}

fn prune_backups(backups_dir: &Path, keep: usize) -> Result<(), DataError> {
    let mut backups: Vec<PathBuf> = std::fs::read_dir(backups_dir)?
        .filter_map(Result::ok)
        .filter(|entry| is_backup_file(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    backups.sort_unstable_by(|left, right| right.cmp(left));
    for stale in backups.into_iter().skip(keep) {
        std::fs::remove_file(stale)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::*;
    use crate::platform::clock::{Clock, FakeClock};
    use crate::platform::logger::silent_logger;
    use crate::testing::{
        MemoryLogger, TemporaryDatabase, temporary_directory, unmigrated_database,
    };

    const SNAPSHOT: &str = include_str!("../../migrations/snapshots/0000_baseline.json");

    fn options<'a>(backups_dir: &'a Path, clock: &Arc<FakeClock>) -> MigrateOptions<'a> {
        MigrateOptions {
            backups_dir,
            logger: silent_logger(),
            clock: clock.clone(),
            keep: BACKUPS_KEPT,
        }
    }

    fn names(connection: &Connection, sql: &str) -> rusqlite::Result<Vec<String>> {
        let mut statement = connection.prepare(sql)?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect()
    }

    fn query(database: &Database, sql: &str) -> Vec<String> {
        database
            .with_connection(|connection| Ok(names(connection, sql)?))
            .expect("query")
    }

    const TABLES: &str = "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name";

    fn tables(database: &Database) -> Vec<String> {
        query(database, TABLES)
    }

    fn journal_names(database: &Database) -> Vec<String> {
        query(database, "SELECT name FROM __migrations ORDER BY id")
    }

    fn fresh() -> (TemporaryDatabase, Arc<FakeClock>) {
        (unmigrated_database(), Arc::new(FakeClock::default()))
    }

    fn bundled() -> Vec<String> {
        MIGRATIONS
            .iter()
            .map(|migration| migration.name.to_owned())
            .collect()
    }

    fn sql_migration(id: u32, name: &'static str, sql: &'static str) -> Migration<'static> {
        Migration { id, name, sql }
    }

    #[test]
    fn the_deepseek_cleanup_removes_its_accounts_and_providers_and_keeps_qwen() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..2], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    "INSERT INTO account (id, adapter, partition, external_id, status, linked_at, updated_at) VALUES                        ('a-gone', 'deepseek-web', 'persist:browser-work', 'u1', 'linked', 1, 1),                        ('a-kept', 'qwen-web', 'persist:browser-work', 'u2', 'linked', 1, 1);                      INSERT INTO provider (id, kind, name, base_url, capabilities, created_at, updated_at, adapter, auth_mode, account_id) VALUES                        ('p-gone', 'openai-compatible', 'DeepSeek', 'https://chat.deepseek.com/api', '[\"text\"]', 1, 1, 'deepseek-web', 'account', 'a-gone'),                        ('p-kept', 'openai-compatible', 'Qwen', 'https://chat.qwen.ai/api/v2', '[\"text\"]', 1, 1, 'qwen-web', 'account', 'a-kept');",
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(&database, "SELECT id FROM account ORDER BY id"),
            ["a-kept"]
        );
        assert_eq!(
            query(&database, "SELECT id FROM provider ORDER BY id"),
            ["p-kept"]
        );
    }

    #[test]
    fn documents_once_converted_to_pdf_leave_the_registry_and_the_rest_stay() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..11], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    "INSERT INTO document (id, content_hash, source_path, kind, mime_type, office_format, page_count, part_count, size_bytes, cached_file, opened_at) VALUES                        ('converted', 'h1', 'C:/a.docx', 'pdf', 'application/pdf', 'docx', 2, 1, 10, 'h1.pdf', 1),                        ('office', 'h2', 'C:/b.xlsx', 'office', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', 'xlsx', NULL, 1, 10, 'h2.xlsx', 1),                        ('pdf', 'h3', 'C:/c.pdf', 'pdf', 'application/pdf', NULL, 2, 1, 10, 'h3.pdf', 1);",
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(&database, "SELECT id FROM document ORDER BY id"),
            ["office", "pdf"]
        );
    }

    #[test]
    fn existing_conversations_keep_the_workspace_tools_they_were_offered() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..12], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    r#"INSERT INTO conversation (id, title, provider_id, model_id, settings, system_prompt, attached_store_ids, enabled_tools, created_at, updated_at) VALUES ('c1', 't', 'p', 'm', '{}', '', '[]', '["mail.send","fs.read"]', 1, 1), ('c2', 't', 'p', 'm', '{}', '', '[]', '[]', 1, 1);"#,
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, &MIGRATIONS[..21], &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(
                &database,
                "SELECT enabled_tools FROM conversation ORDER BY id"
            ),
            [
                r#"["mail.send","fs.read","fs.list","fs.search","fs.apply","code.run","shell.run"]"#,
                r#"["fs.list","fs.read","fs.search","fs.apply","code.run","shell.run"]"#,
            ]
        );
    }

    #[test]
    fn stored_tool_calls_and_transcripts_move_into_their_own_rows_in_order() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..22], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    r#"INSERT INTO conversation (id, title, provider_id, model_id, settings, system_prompt, attached_store_ids, enabled_tools, created_at, updated_at) VALUES ('c1', 't', 'p', 'm', '{}', '', '[]', '[]', 1, 1);
                    INSERT INTO message (id, conversation_id, role, content, citations, tool_calls, tokens_in, tokens_out, usage_estimated, duration_ms, partial, created_at) VALUES ('m1', 'c1', 'assistant', '', '[]', '[{"id":"a"},{"id":"b"}]', 0, 0, 1, 0, 0, 1), ('m2', 'c1', 'user', '', '[]', '[]', 0, 0, 1, 0, 0, 2);
                    INSERT INTO agentic_session (id, goal, status, mode, workspace, provider_id, model_id, tools, budget, usage, transcript, created_at, updated_at) VALUES ('s1', 'g', 'done', 'ask', 'C:/w', 'p', 'm', '[]', '{}', '{}', '[{"role":"user","content":"x"},{"role":"assistant","content":"y"}]', 1, 1);"#,
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(
                &database,
                "SELECT message_id || ':' || idx || ':' || call FROM message_tool_call ORDER BY message_id, idx"
            ),
            [r#"m1:0:{"id":"a"}"#, r#"m1:1:{"id":"b"}"#]
        );
        assert_eq!(
            query(
                &database,
                "SELECT session_id || ':' || seq || ':' || entry FROM agentic_transcript ORDER BY seq"
            ),
            [
                r#"s1:0:{"role":"user","content":"x"}"#,
                r#"s1:1:{"role":"assistant","content":"y"}"#
            ]
        );
        assert!(
            query(
                &database,
                "SELECT name FROM pragma_table_info('message') WHERE name = 'tool_calls' UNION ALL SELECT name FROM pragma_table_info('agentic_session') WHERE name = 'transcript'"
            )
            .is_empty()
        );
    }

    #[test]
    fn merged_and_leaked_tools_leave_every_stored_tool_list() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..20], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    r#"INSERT INTO conversation (id, title, provider_id, model_id, settings, system_prompt, attached_store_ids, enabled_tools, created_at, updated_at) VALUES ('c1', 't', 'p', 'm', '{}', '', '[]', '["fs.read","chat.file","job.run","vector.index","download.fetch","vector.search","mail.send"]', 1, 1), ('c2', 't', 'p', 'm', '{}', '', '[]', '["skill.script","knowledge.read","knowledge.list","knowledge.search"]', 1, 1), ('c3', 't', 'p', 'm', '{}', '', '[]', '["fs.apply"]', 1, 1);
                    INSERT INTO agentic_session (id, goal, status, mode, workspace, provider_id, model_id, tools, budget, usage, created_at, updated_at) VALUES ('s1', 'g', 'done', 'ask', 'C:/w', 'p', 'm', '["skill.read","fs.list","vector.list"]', '{}', '{}', 1, 1);
                    INSERT INTO skill (id, key, name, description, allowed_tools, content_hash, path, created_at, updated_at) VALUES ('k1', 'notes', 'n', 'd', '["skill.read","vector.search","job.run"]', 'h', 'C:/s', 1, 1);
                    INSERT INTO tool (id, key, source, display_name, tier, registered_at, updated_at) VALUES ('t1', 'chat.file', 'builtin', 'chat.file', 'auto', 1, 1), ('t2', 'job.run', 'builtin', 'job.run', 'ask', 1, 1), ('t3', 'fs.read', 'builtin', 'fs.read', 'auto', 1, 1), ('t4', 'vector.search', 'builtin', 'vector.search', 'auto', 1, 1);
                    INSERT INTO tool_reference (id, tool_id, kind, ref_id, name) VALUES ('r1', 't1', 'skill', 'k1', 'notes');
                    INSERT INTO tool_permission (id, subject, scope, tier, granted_at, granted_by) VALUES ('g1', 'chat.file', 'global', 'auto', 1, 'registry'), ('g2', 'job.run', 'global', 'auto', 1, 'user'), ('g3', 'fs.read', 'global', 'auto', 1, 'registry');"#,
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, &MIGRATIONS[..21], &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(
                &database,
                "SELECT enabled_tools FROM conversation ORDER BY id"
            ),
            [
                r#"["fs.read","fs.list","fs.apply","mail.send"]"#,
                r#"["shell.run","knowledge.search"]"#,
                r#"["fs.apply"]"#,
            ]
        );
        assert_eq!(
            query(&database, "SELECT tools FROM agentic_session"),
            [r#"["fs.read","fs.list"]"#]
        );
        assert_eq!(
            query(&database, "SELECT allowed_tools FROM skill"),
            [r#"["fs.read","vector.search"]"#]
        );
        assert_eq!(
            query(&database, "SELECT key FROM tool ORDER BY key"),
            ["fs.read", "vector.search"]
        );
        assert!(query(&database, "SELECT id FROM tool_reference").is_empty());
        assert_eq!(
            query(
                &database,
                "SELECT subject FROM tool_permission ORDER BY subject"
            ),
            ["fs.read", "job.run"]
        );
    }

    #[test]
    fn stored_tool_offers_take_the_new_sources_and_conversations_lose_their_tool_picks() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..25], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    r#"INSERT INTO conversation (id, title, provider_id, model_id, settings, system_prompt, attached_store_ids, enabled_tools, created_at, updated_at) VALUES ('c1', 't', 'p', 'm', '{}', '', '[]', '["fs.read"]', 1, 1);
                    INSERT INTO message (id, conversation_id, role, content, citations, offered_tools, tokens_in, tokens_out, usage_estimated, duration_ms, partial, created_at) VALUES ('m1', 'c1', 'assistant', '', '[]', '[{"key":"fs.read","source":"selected"},{"key":"shell.run","source":"skill","rootsOnly":true},{"key":"knowledge.search","source":"knowledge"},{"key":"chat.memory","source":"system"},{"key":"chat.ask","source":"system"}]', 0, 0, 1, 0, 0, 1), ('m2', 'c1', 'user', '', '[]', '[]', 0, 0, 1, 0, 0, 2);"#,
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(&database, "SELECT offered_tools FROM message ORDER BY id"),
            [
                r#"[{"key":"fs.read","source":"base"},{"key":"shell.run","source":"base"},{"key":"knowledge.search","source":"knowledge"},{"key":"chat.memory","source":"memory"},{"key":"chat.ask","source":"base"}]"#,
                "[]",
            ]
        );
        assert_eq!(
            query(
                &database,
                "SELECT coalesce(command, 'none') FROM message ORDER BY id"
            ),
            ["none", "none"]
        );
        assert!(
            query(
                &database,
                "SELECT name FROM pragma_table_info('conversation') WHERE name = 'enabled_tools'"
            )
            .is_empty()
        );
    }

    #[test]
    fn skills_lose_their_tool_and_model_restrictions_and_the_issues_they_raised() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        migrate(&database, &MIGRATIONS[..21], &options(&backups, &clock)).expect("applies");
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    r#"INSERT INTO skill (id, key, name, description, allowed_tools, model_override, status, issues, content_hash, path, created_at, updated_at) VALUES ('k1', 'tools', 'n', 'd', '["mail.send"]', '{"providerId":"p","modelId":"m"}', 'invalid', '[{"field":"allowedTools.0","rule":"unknownTool"}]', 'h', 'C:/s/tools', 1, 1), ('k2', 'mixed', 'n', 'd', '["mail.send"]', NULL, 'invalid', '[{"field":"allowedTools.0","rule":"toolOff"},{"field":"files","rule":"path"}]', 'h', 'C:/s/mixed', 1, 1), ('k3', 'clean', 'n', 'd', '[]', NULL, 'ready', '[]', 'h', 'C:/s/clean', 1, 1);
                    INSERT INTO tool (id, key, source, display_name, tier, registered_at, updated_at) VALUES ('t1', 'mail.send', 'builtin', 'mail.send', 'auto', 1, 1);
                    INSERT INTO tool_reference (id, tool_id, kind, ref_id, name) VALUES ('r1', 't1', 'skill', 'k1', 'tools'), ('r2', 't1', 'scenario', 's1', 'flow');"#,
                )?;
                Ok(())
            })
            .expect("seeds");
        migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
        assert_eq!(
            query(
                &database,
                "SELECT key || ' ' || status || ' ' || issues FROM skill ORDER BY id"
            ),
            [
                "tools ready []",
                r#"mixed invalid [{"field":"files","rule":"path"}]"#,
                "clean ready []",
            ]
        );
        assert_eq!(query(&database, "SELECT id FROM tool_reference"), ["r2"]);
        assert!(
            query(
                &database,
                "SELECT name FROM pragma_table_info('skill') WHERE name IN ('allowed_tools', 'model_override')"
            )
            .is_empty()
        );
    }

    #[test]
    fn the_baseline_applies_once_to_an_empty_file_and_is_a_no_op_afterwards() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        let first = migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
        assert_eq!(
            first,
            MigrateReport {
                applied: bundled(),
                backup: None,
            }
        );
        assert!(tables(&database).contains(&"setting".to_owned()));
        assert_eq!(journal_names(&database), bundled());

        let second = migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("no-op");
        assert_eq!(second, MigrateReport::default());
        assert!(!backups.exists());
    }

    #[test]
    fn the_baseline_schema_equals_the_frozen_snapshot() {
        let (database, clock) = fresh();
        migrate(
            &database,
            &MIGRATIONS[..1],
            &options(&database.backups_dir(), &clock),
        )
        .expect("applies");
        let actual = database
            .with_connection(|connection| {
                let mut statement = connection.prepare(
                    "SELECT type, name, tbl_name, sql FROM sqlite_master \
                     WHERE name NOT LIKE 'sqlite_%' AND name <> '__migrations' ORDER BY rowid",
                )?;
                let rows = statement
                    .query_map([], |row| {
                        Ok(json!({
                            "type": row.get::<_, String>(0)?,
                            "name": row.get::<_, String>(1)?,
                            "tbl_name": row.get::<_, String>(2)?,
                            "sql": row.get::<_, Option<String>>(3)?,
                        }))
                    })?
                    .collect::<rusqlite::Result<Vec<Value>>>()?;
                Ok(rows)
            })
            .expect("reads the schema");
        let expected: Vec<Value> = serde_json::from_str(SNAPSHOT).expect("a snapshot");
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(actual, expected, "schema entry {index}");
        }
        assert_eq!(actual.len(), expected.len());
    }

    #[test]
    fn the_baseline_keeps_every_foreign_key_and_has_no_violations() {
        let (database, clock) = fresh();
        migrate(
            &database,
            MIGRATIONS,
            &options(&database.backups_dir(), &clock),
        )
        .expect("applies");
        let references = query(
            &database,
            "SELECT m.name || '.' || p.\"from\" || '->' || p.\"table\" || '.' || coalesce(p.\"to\", '') \
             FROM sqlite_master m, pragma_foreign_key_list(m.name) p \
             WHERE m.type = 'table' ORDER BY m.name, p.id, p.seq",
        );
        assert!(
            references.contains(&"graph_edge.relation_type_id->graph_relation_type.id".to_owned())
        );
        assert!(references.contains(&"provider.account_id->account.id".to_owned()));
        assert!(references.contains(&"message.run_id->run.id".to_owned()));
        assert!(query(&database, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    }

    #[test]
    fn already_applied_migrations_are_skipped_and_a_populated_database_is_backed_up() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        let first = sql_migration(
            0,
            "0000_first",
            "CREATE TABLE first (id INTEGER PRIMARY KEY);",
        );
        let second = sql_migration(
            1,
            "0001_second",
            "CREATE TABLE second (id INTEGER PRIMARY KEY);",
        );
        migrate(&database, &[first], &options(&backups, &clock)).expect("applies");
        let report =
            migrate(&database, &[first, second], &options(&backups, &clock)).expect("applies");
        assert_eq!(report.applied, ["0001_second"]);
        let backup = report.backup.expect("a backup");
        assert_eq!(backup, backup_file_path(&backups, clock.now()));
        let copy = Database::open(&backup, silent_logger()).expect("opens the backup");
        assert_eq!(tables(&copy), ["__migrations", "first"]);
        assert_eq!(journal_names(&database), ["0000_first", "0001_second"]);
    }

    #[test]
    fn a_backup_is_written_before_migrating_and_only_the_last_three_are_kept() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        let mut written = Vec::new();
        for run in 0..BACKUPS_KEPT + 2 {
            clock.advance(60_000);
            let report =
                migrate(&database, MIGRATIONS, &options(&backups, &clock)).expect("applies");
            assert_eq!(report.applied, bundled());
            match report.backup {
                None => assert_eq!(run, 0),
                Some(backup) => written.push(backup),
            }
            database
                .with_connection(|connection| {
                    connection.execute_batch("PRAGMA foreign_keys = OFF")?;
                    for table in names(connection, TABLES)? {
                        connection.execute_batch(&format!("DROP TABLE \"{table}\""))?;
                    }
                    connection.execute_batch(
                        "PRAGMA foreign_keys = ON; CREATE TABLE keepalive (id INTEGER PRIMARY KEY)",
                    )?;
                    Ok(())
                })
                .expect("resets the schema");
        }
        assert_eq!(written.len(), BACKUPS_KEPT + 1);
        let mut kept: Vec<PathBuf> = std::fs::read_dir(&backups)
            .expect("backups")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        kept.sort();
        assert_eq!(kept, written[written.len() - BACKUPS_KEPT..]);
    }

    #[test]
    fn a_broken_migration_fails_and_leaves_the_database_untouched() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        let logger = MemoryLogger::shared();
        let broken = sql_migration(
            0,
            "0000_setting",
            "CREATE TABLE setting (key TEXT);\nCREATE TABLE (this is not sql;\n",
        );
        let failure = migrate(
            &database,
            &[broken],
            &MigrateOptions {
                logger: logger.clone(),
                ..options(&backups, &clock)
            },
        )
        .expect_err("fails");
        assert_eq!(failure.backups_dir, backups);
        assert!(
            failure.message.contains("syntax error"),
            "{}",
            failure.message
        );
        assert!(tables(&database).is_empty());
        assert_eq!(logger.matching("Migration failed").len(), 1);
    }

    #[test]
    fn a_failure_keeps_earlier_migrations_and_rolls_back_only_the_failing_one() {
        let (database, clock) = fresh();
        let migrations = [
            sql_migration(0, "0000_ok", "CREATE TABLE ok (id INTEGER PRIMARY KEY);"),
            sql_migration(
                1,
                "0001_broken",
                "CREATE TABLE half (id INTEGER PRIMARY KEY); INSERT INTO nowhere VALUES (1);",
            ),
        ];
        migrate(
            &database,
            &migrations,
            &options(&database.backups_dir(), &clock),
        )
        .expect_err("fails");
        assert_eq!(tables(&database), ["__migrations", "ok"]);
        assert_eq!(journal_names(&database), ["0000_ok"]);
    }

    #[test]
    fn an_applied_migration_whose_checksum_changed_is_a_hard_failure() {
        let (database, clock) = fresh();
        let backups = database.backups_dir();
        let original = sql_migration(
            0,
            "0000_first",
            "CREATE TABLE first (id INTEGER PRIMARY KEY);",
        );
        migrate(&database, &[original], &options(&backups, &clock)).expect("applies");
        let edited = Migration {
            sql: "CREATE TABLE first (id INTEGER PRIMARY KEY, extra TEXT);",
            ..original
        };
        let pending = sql_migration(
            1,
            "0001_second",
            "CREATE TABLE second (id INTEGER PRIMARY KEY);",
        );
        let failure =
            migrate(&database, &[edited, pending], &options(&backups, &clock)).expect_err("fails");
        assert!(
            failure.message.contains("0000_first"),
            "{}",
            failure.message
        );
        assert_eq!(tables(&database), ["__migrations", "first"]);
        assert!(!backups.exists());
    }

    #[test]
    fn line_endings_do_not_change_a_checksum() {
        let unix = sql_migration(0, "0000_a", "CREATE TABLE a (\n  id INTEGER\n);\n");
        let windows = Migration {
            sql: "CREATE TABLE a (\r\n  id INTEGER\r\n);\r\n",
            ..unix
        };
        assert_eq!(unix.checksum(), windows.checksum());
        assert_eq!(unix.checksum().len(), 64);
        let other = Migration {
            sql: "CREATE TABLE b (id INTEGER);",
            ..unix
        };
        assert_ne!(unix.checksum(), other.checksum());
    }

    #[test]
    fn the_embedded_migrations_are_ordered_and_uniquely_named() {
        assert!(check_order(MIGRATIONS).is_ok());
        assert_eq!(MIGRATIONS[0].name, "0000_baseline");
        let unordered = [MIGRATIONS[0], sql_migration(0, "0000_again", "")];
        assert!(check_order(&unordered).is_err());
        let renamed = [MIGRATIONS[0], sql_migration(1, "0000_baseline", "")];
        assert!(check_order(&renamed).is_err());
    }

    #[test]
    fn a_corrupt_database_file_is_refused_at_open_as_a_migration_failure() {
        let directory = temporary_directory("studio-corrupt-");
        let file = directory.join("studio.sqlite");
        std::fs::write(&file, "not a database at all, just junk bytes").expect("writes");
        let backups = directory.join("backups");
        let failure = prepare_database(&PrepareOptions {
            file: &file,
            backups_dir: &backups,
            logger: silent_logger(),
            clock: Arc::new(FakeClock::default()),
        })
        .err()
        .expect("fails");
        assert_eq!(failure.backups_dir, backups);
    }

    #[test]
    fn prepare_database_opens_and_migrates_in_one_step() {
        let directory = temporary_directory("studio-prepare-");
        let file = directory.join("studio.sqlite");
        let (database, report) = prepare_database(&PrepareOptions {
            file: &file,
            backups_dir: &directory.join("backups"),
            logger: silent_logger(),
            clock: Arc::new(FakeClock::default()),
        })
        .expect("prepares");
        assert_eq!(report.applied, bundled());
        assert_eq!(database.file(), file);
        assert!(file.exists());
        assert!(query(&database, "SELECT key FROM setting").is_empty());
    }

    #[test]
    fn a_migration_failure_maps_to_db_error_with_the_backups_dir() {
        let error = AppError::from(MigrationFailed::new("boom", Path::new("backups")));
        assert_eq!(error.code, AppErrorCode::DbError);
        assert_eq!(error.message, "boom");
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("backupsDir").cloned()),
            Some(json!("backups"))
        );
    }
}
