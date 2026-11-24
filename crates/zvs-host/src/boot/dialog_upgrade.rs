use std::fs;
use std::path::Path;
use std::sync::Arc;

use rusqlite::{Connection, OpenFlags};
use uuid::Uuid;

use super::{BootError, StepContext, StepProgress, project_struct, validate_dialogs};
use crate::data::repositories::ChatRepository;
use crate::data::{DataError, MIGRATIONS, PrepareOptions, prepare_with};
use crate::dialog_history::{DialogChange, DialogHistory, HISTORY_FILE};
use crate::platform::{clock::SystemClock, logger::silent_logger};

pub(super) fn upgrade(
    context: &StepContext<'_>,
    report: &dyn Fn(StepProgress),
) -> Result<(), BootError> {
    let paths = project_struct::layout(context.root);
    if !paths.db_path.is_file() {
        return validate_dialogs(context, report);
    }
    let source = Connection::open_with_flags(&paths.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(failed)?;
    let projected = source.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'dialog_checkpoint')", [], |row| row.get::<_, bool>(0),
    ).map_err(failed)?;
    if projected {
        return validate_dialogs(context, report);
    }
    fs::create_dir_all(&paths.staging_dir).map_err(failed)?;
    let staging = tempfile::Builder::new()
        .prefix("dialogs-v1-")
        .tempdir_in(&paths.staging_dir)
        .map_err(failed)?;
    let snapshot = staging.path().join("legacy.sqlite");
    source
        .execute("VACUUM INTO ?1", [snapshot.to_string_lossy().as_ref()])
        .map_err(failed)?;
    drop(source);

    let (database, _) = prepare_with(
        &PrepareOptions {
            file: &snapshot,
            backups_dir: &staging.path().join("backups"),
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
        },
        &MIGRATIONS[..MIGRATIONS.partition_point(|migration| migration.id < 46)],
    )
    .map_err(failed)?;
    let staging_root = staging.path().join("history");
    let history = DialogHistory::new(staging_root.clone());
    let current = DialogHistory::new(paths.conversations_dir.clone());
    if !current.dialogs().map_err(failed)?.is_empty() {
        super::validate_history(&current)?;
    }
    let (converted, unsupported) = database
        .with_connection(|connection| stage(connection, &current, &history, report))
        .map_err(failed)?;
    super::validate_history(&history)?;
    for id in converted {
        let _writer = current.writer(&id).map_err(failed)?;
        let destination = paths.conversations_dir.join(&id);
        fs::create_dir_all(&destination).map_err(failed)?;
        let target = destination.join(HISTORY_FILE);
        if current.replay(&id, |_| Ok(())).map_err(failed)? == 0 {
            fs::rename(staging_root.join(id).join(HISTORY_FILE), target).map_err(failed)?;
        }
    }
    super::validate_history(&current)?;
    for id in unsupported {
        quarantine(&paths.conversations_dir, &paths.backups_dir, &id)?;
    }
    Ok(())
}

fn stage(
    connection: &Connection,
    current: &DialogHistory,
    staged: &DialogHistory,
    report: &dyn Fn(StepProgress),
) -> Result<(Vec<String>, Vec<String>), DataError> {
    let mut query = connection.prepare("SELECT id FROM conversation ORDER BY id")?;
    let ids = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let total = ids.len() as u64;
    report(StepProgress { done: 0, total });
    let chat = ChatRepository::new(connection);
    let mut converted = Vec::new();
    let mut unsupported = Vec::new();
    for (index, id) in ids.into_iter().enumerate() {
        if crate::conversations::is_owner(&id) && current.replay(&id, |_| Ok(()))? > 0 {
            report(StepProgress {
                done: index as u64 + 1,
                total,
            });
            continue;
        }
        match records(connection, &chat, &id) {
            Ok(changes) if !changes.is_empty() => {
                staged.writer(&id)?.append(0, &changes)?;
                converted.push(id);
            }
            Ok(_)
            | Err(
                rusqlite::Error::FromSqlConversionFailure(..)
                | rusqlite::Error::InvalidColumnType(..)
                | rusqlite::Error::IntegralValueOutOfRange(..),
            ) => unsupported.push(id),
            Err(error) => return Err(error.into()),
        }
        report(StepProgress {
            done: index as u64 + 1,
            total,
        });
    }
    Ok((converted, unsupported))
}

fn records(
    connection: &Connection,
    chat: &ChatRepository<'_>,
    id: &str,
) -> rusqlite::Result<Vec<DialogChange>> {
    if !canonical_id(id) {
        return Ok(Vec::new());
    }
    let malformed_calls = connection.prepare_cached(
        "SELECT EXISTS(SELECT 1 FROM message JOIN message_tool_call ON message_tool_call.message_id = message.id
         WHERE message.conversation_id = ?1 AND NOT json_valid(message_tool_call.call))",
    )?.query_row([id], |row| row.get::<_, bool>(0))?;
    if malformed_calls {
        return Ok(Vec::new());
    }
    let metadata = chat.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    if !canonical_id(&metadata.provider_id)
        || metadata
            .attached_store_ids
            .iter()
            .any(|id| !canonical_id(id))
    {
        return Ok(Vec::new());
    }
    let messages = chat.messages(id)?;
    let compactions = chat.compactions(id)?;
    if messages.iter().any(|row| !canonical_id(&row.id))
        || compactions.iter().any(|row| !canonical_id(&row.id))
    {
        return Ok(Vec::new());
    }
    Ok(std::iter::once(DialogChange::Metadata(Box::new(metadata)))
        .chain(
            messages
                .into_iter()
                .map(|row| DialogChange::Message(Box::new(row))),
        )
        .chain(
            compactions
                .into_iter()
                .map(|row| DialogChange::Compaction(Box::new(row))),
        )
        .collect())
}

fn canonical_id(id: &str) -> bool {
    Uuid::parse_str(id).is_ok_and(|parsed| parsed.hyphenated().to_string() == id)
}

fn quarantine(root: &Path, backups: &Path, id: &str) -> Result<(), BootError> {
    if !crate::conversations::is_owner(id) {
        return Ok(());
    }
    let source = root.join(id);
    if !source.try_exists().map_err(failed)? {
        return Ok(());
    }
    let backups = backups.join("dialogs-layout-v1");
    fs::create_dir_all(&backups).map_err(failed)?;
    fs::rename(source, backups.join(id)).map_err(failed)
}

fn failed(error: impl std::fmt::Display) -> BootError {
    BootError::DialogHistory(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::{Boot, BootDecision};
    use crate::data::{Repositories, prepare_database};
    use crate::testing::temporary_directory;
    use zvs_contract::layout::StateFile;

    const DIALOG: &str = "0199aa11-2222-7111-8111-111111111111";
    const MESSAGE: &str = "0199aa11-3333-7111-8111-111111111111";
    const PROVIDER: &str = "0199aa11-4444-7111-8111-111111111111";
    const RUN: &str = "0199aa11-5555-7111-8111-111111111111";

    #[test]
    fn v1_dialogs_are_converted_once_and_keep_their_owned_files_and_memory() {
        let root = temporary_directory("dialog-layout-v1");
        let paths = project_struct::layout(root.path());
        let options = PrepareOptions {
            file: &paths.db_path,
            backups_dir: &paths.backups_dir,
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
        };
        let (database, _) = prepare_with(
            &options,
            &MIGRATIONS[..MIGRATIONS.partition_point(|migration| migration.id < 45)],
        )
        .expect("prepares v1 database");
        database.with_connection(|connection| {
            connection.execute("INSERT INTO conversation (id,title,provider_id,model_id,settings,system_prompt,attached_store_ids,created_at,updated_at) VALUES (?1,'Supported',?2,'m','{}','','[]',1,2),('unsupported','Retired',?2,'m','{}','','[]',1,2)", [DIALOG, PROVIDER])?;
            connection.execute("INSERT INTO run (id,kind,subject_id,status,graph,stream_id,concurrency,created_at,finished_at) VALUES (?1,'chat',?2,'succeeded','{}','test-stream',1,1,2)", rusqlite::params![RUN, DIALOG])?;
            connection.execute("INSERT INTO message (id,conversation_id,role,content,citations,run_id,tokens_in,tokens_out,duration_ms,created_at) VALUES (?1,?2,'assistant','Kept history','[]',?3,2,1,3,2)", rusqlite::params![MESSAGE, DIALOG, RUN])?;
            connection.execute("INSERT INTO memory VALUES ('owned','conversation',?1,'kept memory','user',1,1)", [DIALOG])?;
            connection.execute("INSERT INTO access_grant (id,resource_kind,resource,action,decision,scope,owner,granted_by,granted_at) VALUES ('owned','tool','fs.read','call','allow','conversation',?1,'user',1)", [DIALOG])?;
            Ok(())
        }).expect("seeds legacy history");
        drop(database);
        fs::write(
            root.join("aistudio.toml"),
            "[schemas]\nproject_struct = 1\n",
        )
        .expect("records v1");
        let owned = paths.conversations_dir.join(DIALOG).join("work");
        let unsupported = paths.conversations_dir.join("unsupported");
        fs::create_dir_all(&owned).expect("creates work directory");
        fs::create_dir(&unsupported).expect("creates unsupported directory");
        fs::write(owned.join("file.txt"), "keep").expect("writes owned file");
        fs::write(unsupported.join("old.txt"), "backup").expect("writes unsupported file");

        let malformed = paths.conversations_dir.join("broken");
        fs::create_dir(&malformed).expect("creates interrupted history directory");
        fs::write(malformed.join(HISTORY_FILE), b"{}\n")
            .expect("blocks publication of the layout stamp");
        let boot = Boot::new(root.path(), None);
        let decision = boot.inspect().expect("inspects legacy layout");
        assert!(boot.run(&decision, 2, &|_| {}).is_err());
        let log = paths.conversations_dir.join(DIALOG).join(HISTORY_FILE);
        assert!(!log.exists());
        fs::remove_file(malformed.join(HISTORY_FILE)).expect("clears corrupt fixture");
        upgrade(
            &StepContext {
                root: root.path(),
                legacy: None,
            },
            &|_| {},
        )
        .expect("publishes before the version is written");
        let converted = fs::read(&log).expect("first history was published");
        boot.run(&decision, 3, &|_| {})
            .expect("retries the conversion");
        assert_eq!(fs::read(&log).expect("converted history"), converted);
        assert_eq!(boot.inspect().expect("upgraded"), BootDecision::Ready);
        assert_eq!(
            StateFile::read(root.path())
                .expect("reads version")
                .expect("state file")
                .version(zvs_contract::layout::PROJECT_STRUCT),
            2
        );
        assert_eq!(
            fs::read_to_string(owned.join("file.txt")).expect("retains owned files"),
            "keep"
        );
        assert!(!unsupported.exists());
        assert!(
            paths
                .backups_dir
                .join("dialogs-layout-v1/unsupported/old.txt")
                .is_file()
        );

        let (mut database, report) =
            prepare_database(&options).expect("replaces the old history tables");
        assert!(
            report
                .backup
                .expect("preserves the legacy source")
                .is_file()
        );
        database
            .enable_dialog_history(paths.conversations_dir)
            .expect("rebuilds the dialog index");
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                assert_eq!(
                    repositories
                        .chat
                        .list()?
                        .iter()
                        .map(|row| row.id.as_str())
                        .collect::<Vec<_>>(),
                    [DIALOG]
                );
                let restored = repositories
                    .chat
                    .get_message(MESSAGE)?
                    .expect("restores message");
                assert_eq!(restored.content, "Kept history");
                assert_eq!(restored.run_id.as_deref(), Some(RUN));
                assert_eq!(
                    repositories.memory.list_conversation(DIALOG)?[0].content,
                    "kept memory"
                );
                assert_eq!(
                    connection.query_row(
                        "SELECT owner FROM access_grant WHERE id = 'owned'",
                        [],
                        |row| row.get::<_, String>(0)
                    )?,
                    DIALOG
                );
                Ok(())
            })
            .expect("uses the converted history");
    }
}
