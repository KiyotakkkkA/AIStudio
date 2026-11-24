use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use rusqlite::{Connection, OptionalExtension, params};

use super::DataError;
use super::repositories::ChatRepository;
use super::schema::CompactionRow;
use crate::dialog_history::{DialogChange, DialogHistory, DialogWriter};

pub(crate) struct DialogJournal {
    history: DialogHistory,
    leases: Mutex<HashMap<String, Weak<DialogWriter>>>,
}

impl DialogJournal {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            history: DialogHistory::new(root),
            leases: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn reserve(&self, id: &str) -> std::io::Result<Arc<DialogWriter>> {
        let mut leases = self.leases.lock().unwrap_or_else(PoisonError::into_inner);
        leases.retain(|_, lease| lease.strong_count() > 0);
        let writer = Arc::new(self.history.writer(id)?);
        leases.insert(id.to_owned(), Arc::downgrade(&writer));
        Ok(writer)
    }

    fn writer(&self, id: &str) -> std::io::Result<Arc<DialogWriter>> {
        let leases = self.leases.lock().unwrap_or_else(PoisonError::into_inner);
        match leases.get(id).and_then(Weak::upgrade) {
            Some(writer) => Ok(writer),
            None => Ok(Arc::new(self.history.writer(id)?)),
        }
    }

    pub(super) fn rebuild(&self, connection: &Connection) -> Result<(), DataError> {
        Self::rebuild_projection(&self.history, connection)
    }

    pub(crate) fn rebuild_projection(
        history: &DialogHistory,
        connection: &Connection,
    ) -> Result<(), DataError> {
        connection.execute_batch(concat!(
            "DROP TRIGGER IF EXISTS message_search_insert;
             DROP TRIGGER IF EXISTS message_search_delete;
             DROP TRIGGER IF EXISTS message_search_update;
             DROP TABLE IF EXISTS message_search;",
            include_str!("dialogs_schema.sql"),
            "DELETE FROM chat_compaction;
             DELETE FROM message_tool_call;
             DELETE FROM message;
             DELETE FROM dialog_checkpoint;",
            include_str!("dialogs_search.sql"),
        ))?;
        let chat = ChatRepository::new(connection);
        let mut recovered = HashSet::new();
        for id in history.dialogs()? {
            let mut live = false;
            let offset = history.replay(&id, |changes| {
                for change in changes {
                    match &change {
                        DialogChange::Metadata(row) if row.id == id => live = true,
                        DialogChange::Delete => live = false,
                        _ if !live => {
                            return Err(std::io::Error::other("Dialog record has no metadata"));
                        }
                        _ => {}
                    }
                    apply(connection, &chat, &id, &change).map_err(std::io::Error::other)?;
                }
                Ok(())
            })?;
            if offset > 0 {
                set_checkpoint(connection, &id, offset)?;
            }
            if live {
                recovered.insert(id);
            }
        }
        let mut query = connection.prepare("SELECT id FROM conversation")?;
        let indexed = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for id in indexed {
            if !recovered.contains(&id) {
                chat.remove(&id)?;
            }
        }
        Ok(())
    }

    pub(super) fn install(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch(include_str!("dialogs_capture.sql"))
    }

    pub(super) fn clean_deleted(&self, connection: &Connection) -> rusqlite::Result<()> {
        let mut query = connection.prepare("SELECT id FROM dialog_checkpoint WHERE NOT EXISTS (SELECT 1 FROM conversation WHERE conversation.id = dialog_checkpoint.id)")?;
        for id in query.query_map([], |row| row.get::<_, String>(0))? {
            if let Ok(writer) = self.history.writer(&id?) {
                writer.clean_deleted_folder();
            }
        }
        Ok(())
    }

    pub(super) fn publish(&self, connection: &Connection) -> Result<Vec<Published>, DataError> {
        let batches = changes(connection)?;
        if batches.is_empty() {
            return Ok(Vec::new());
        }
        connection.execute_batch("DELETE FROM dialog_changes; DELETE FROM dialog_stream;")?;
        let mut staged = Vec::with_capacity(batches.len());
        for (id, changes) in batches {
            let writer = self.writer(&id)?;
            let offset = read_checkpoint(connection, &id)?;
            staged.push((
                id,
                changes,
                Published {
                    writer,
                    offset,
                    end: offset,
                },
            ));
        }
        let mut published = Vec::with_capacity(staged.len());
        for (id, changes, mut entry) in staged {
            let result = (|| {
                entry.end = entry.writer.append(entry.offset, &changes)?;
                set_checkpoint(connection, &id, entry.end)?;
                Ok::<_, DataError>(())
            })();
            published.push(entry);
            if let Err(error) = result {
                Self::rollback(&published)?;
                return Err(error);
            }
        }
        Ok(published)
    }

    pub(super) fn rollback(published: &[Published]) -> Result<(), DataError> {
        for entry in published.iter().rev() {
            if entry.end != entry.offset {
                entry.writer.rollback(entry.offset, entry.end)?;
            }
        }
        Ok(())
    }
}

pub(super) struct Published {
    writer: Arc<DialogWriter>,
    offset: u64,
    end: u64,
}

fn read_checkpoint(connection: &Connection, id: &str) -> Result<u64, DataError> {
    let offset: i64 = connection
        .query_row(
            "SELECT offset FROM dialog_checkpoint WHERE id = ?1",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or_default();
    Ok(u64::try_from(offset).map_err(std::io::Error::other)?)
}

fn set_checkpoint(connection: &Connection, id: &str, offset: u64) -> Result<(), DataError> {
    let offset = i64::try_from(offset).map_err(std::io::Error::other)?;
    connection.execute(
        "INSERT INTO dialog_checkpoint (id, offset) VALUES (?1, ?2)
         ON CONFLICT (id) DO UPDATE SET offset = excluded.offset",
        params![id, offset],
    )?;
    Ok(())
}

fn changes(connection: &Connection) -> Result<BTreeMap<String, Vec<DialogChange>>, DataError> {
    let chat = ChatRepository::new(connection);
    let mut query = connection.prepare_cached(
        "SELECT DISTINCT owner, kind, id, position FROM dialog_changes
         ORDER BY owner, CASE kind WHEN 'delete_dialog' THEN 0 WHEN 'metadata' THEN 1
              WHEN 'message' THEN 2 WHEN 'tool_call' THEN 3 WHEN 'remove_compaction' THEN 4
              WHEN 'compaction' THEN 5 WHEN 'remove_message' THEN 6 END, id, position",
    )?;
    let mut pending = query.query([])?;
    let mut batches: BTreeMap<String, Vec<DialogChange>> = BTreeMap::new();
    while let Some(row) = pending.next()? {
        let owner: String = row.get(0)?;
        let kind: String = row.get(1)?;
        let id: String = row.get(2)?;
        let change = match kind.as_str() {
            "delete_dialog" => Some(DialogChange::Delete),
            "metadata" => chat
                .get(&id)?
                .map(|row| DialogChange::Metadata(Box::new(row))),
            "message" => chat
                .get_message(&id)?
                .map(|row| DialogChange::Message(Box::new(row))),
            "tool_call" => {
                let index: u32 = row.get(3)?;
                let call = connection
                    .query_row(
                        "SELECT call FROM message_tool_call WHERE message_id = ?1 AND idx = ?2",
                        params![id, index],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                call.map(|call| serde_json::from_str(&call))
                    .transpose()
                    .map_err(std::io::Error::from)?
                    .map(|call| DialogChange::ToolCall { id, index, call })
            }
            "remove_compaction" => Some(DialogChange::RemoveCompaction { id }),
            "compaction" => connection
                .query_row(
                    &format!(
                        "SELECT {} FROM chat_compaction WHERE id = ?1",
                        CompactionRow::COLUMNS
                    ),
                    [&id],
                    CompactionRow::from_row,
                )
                .optional()?
                .map(|row| DialogChange::Compaction(Box::new(row))),
            "remove_message" => Some(DialogChange::RemoveMessage { id }),
            _ => return Err(rusqlite::Error::InvalidQuery.into()),
        };
        if let Some(change) = change {
            let batch = batches.entry(owner).or_default();
            if !matches!(batch.first(), Some(DialogChange::Delete)) {
                batch.push(change);
            }
        }
    }
    let mut stream = connection.prepare_cached(
        "SELECT owner, id, content, reasoning, tokens_out, duration_ms FROM dialog_stream AS stream
         WHERE NOT EXISTS (SELECT 1 FROM dialog_changes AS change WHERE change.owner = stream.owner
             AND (change.kind = 'delete_dialog' OR (change.kind IN ('message', 'remove_message') AND change.id = stream.id)))
         ORDER BY stream.rowid",
    )?;
    let mut deltas = stream.query([])?;
    while let Some(row) = deltas.next()? {
        batches
            .entry(row.get(0)?)
            .or_default()
            .push(DialogChange::StreamDelta {
                id: row.get(1)?,
                content: row.get(2)?,
                reasoning: row.get(3)?,
                tokens_out: row.get(4)?,
                duration_ms: row.get(5)?,
            });
    }
    Ok(batches)
}

fn apply(
    connection: &Connection,
    chat: &ChatRepository<'_>,
    owner: &str,
    change: &DialogChange,
) -> rusqlite::Result<()> {
    match change {
        DialogChange::Metadata(row) if row.id == owner => chat.project_metadata(row),
        DialogChange::Message(row) if row.conversation_id == owner => {
            if chat
                .get_message(&row.id)?
                .is_some_and(|message| message.conversation_id != owner)
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            chat.project_message(row)
        }
        DialogChange::StreamDelta {
            id,
            content,
            reasoning,
            tokens_out,
            duration_ms,
        } => {
            let updated = connection.execute(
                "UPDATE message SET content = content || ?1, reasoning = reasoning || ?2,
                 tokens_out = ?3, duration_ms = ?4 WHERE id = ?5 AND conversation_id = ?6",
                params![content, reasoning, tokens_out, duration_ms, id, owner],
            )?;
            if updated != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            Ok(())
        }
        DialogChange::ToolCall { id, index, call } => {
            let present = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM message WHERE id = ?1 AND conversation_id = ?2)",
                params![id, owner],
                |row| row.get::<_, bool>(0),
            )?;
            if !present {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            chat.put_tool_calls(id, &[(*index as usize, call)])
        }
        DialogChange::RemoveMessage { id } => {
            connection.execute(
                "DELETE FROM message WHERE id = ?1 AND conversation_id = ?2",
                params![id, owner],
            )?;
            Ok(())
        }
        DialogChange::Compaction(row) if row.conversation_id == owner => {
            let present = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM message WHERE id = ?1 AND conversation_id = ?2)",
                params![row.boundary_message_id, owner],
                |row| row.get::<_, bool>(0),
            )?;
            if !present {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            chat.add_compaction(row)
        }
        DialogChange::RemoveCompaction { id } => {
            chat.remove_compaction(owner, id)?;
            Ok(())
        }
        DialogChange::Delete => chat.remove(owner),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
