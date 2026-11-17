use std::sync::Arc;

use serde_json::{Value, json};
use zvs_contract::TransferCategory::{Connections, Conversations, Memory, Settings};

use super::*;
use crate::data::repositories::data_transfer::{Record, set};
use crate::platform::clock::FakeClock;
use crate::platform::paths::PathEnvironment;
use crate::testing::{
    TemporaryDatabase, TemporaryDirectory, temporary_database, temporary_directory,
};

const PROVIDER: &str = "0199aa11-1111-7111-8111-111111111111";
const CONVERSATION: &str = "0199aa11-2222-7111-8111-111111111111";
const MESSAGE: &str = "0199aa11-3333-7111-8111-111111111111";
const MODEL: &str = "0199aa11-4444-7111-8111-111111111111";
const REMOVED_PROVIDER: &str = "0199aa11-5555-7111-8111-111111111111";

struct Fixture {
    _directory: TemporaryDirectory,
    paths: StudioPaths,
    database: TemporaryDatabase,
}

fn fixture() -> Fixture {
    let directory = temporary_directory("data-transfer-");
    let paths = StudioPaths::resolve(&PathEnvironment {
        state_dir: directory.join("state"),
        resource_dir: directory.join("resources"),
        executable_dir: directory.join("bin"),
        packaged: false,
    });
    Fixture {
        _directory: directory,
        paths,
        database: temporary_database(),
    }
}

fn seed(fixture: &Fixture) {
    fixture.database.with_connection(|connection| {
        connection.execute("INSERT INTO provider (id,kind,name,base_url,capabilities,cap_text,settings,created_at,updated_at) VALUES (?1,'ollama','Local','http://localhost:11434','[\"text\"]',1,'{\"timeoutSeconds\":45,\"privateValue\":\"hidden\"}',1,2)", [PROVIDER])?;
        connection.execute("INSERT INTO conversation (id,title,provider_id,model_id,settings,system_prompt,attached_store_ids,created_at,updated_at) VALUES (?1,'Dialog',?2,'fake-model','{}','prompt','[]',1,2)", rusqlite::params![CONVERSATION, PROVIDER])?;
        connection.execute("INSERT INTO message (id,conversation_id,role,content,sources,tokens_in,tokens_out,duration_ms,created_at) VALUES (?1,?2,'user',?3,'[]',0,0,0,1)", rusqlite::params![MESSAGE, CONVERSATION, format!("Literal {PROVIDER}")])?;
        connection.execute("INSERT INTO setting (key,value,updated_at) VALUES ('ui.theme','\"dark\"',1),('browser.cookies','\"private\"',1)", [])?;
        connection.execute("INSERT INTO model (id,provider_id,external_id,display_name,context_window,max_output,discovered_at) VALUES (?1,?2,?1,'Remote model',0,0,1)", rusqlite::params![MODEL,PROVIDER])?;
        connection.execute("UPDATE provider SET default_model_id=?1 WHERE id=?2", rusqlite::params![MODEL,PROVIDER])?;
        Ok(())
    }).expect("seeded");
    let folder = fixture
        .paths
        .conversations_dir
        .join(CONVERSATION)
        .join("attachments");
    fs::create_dir_all(&folder).expect("attachments folder");
    fs::write(folder.join("sample.txt"), b"attachment").expect("attachment");
}

fn package(
    fixture: &Fixture,
    categories: Vec<zvs_contract::TransferCategory>,
    blobs: &Path,
) -> Package {
    fs::create_dir_all(blobs).expect("blobs");
    let records = fixture
        .database
        .with_connection(|connection| TransferRepository::new(connection).export(&categories))
        .expect("exported records");
    let mut package = Package {
        format: "zvs-dbox".to_owned(),
        version: 1,
        producer_version: "test".to_owned(),
        created_at: 1,
        categories,
        records,
        files: Vec::new(),
        missing_files: false,
    };
    collect_files(&fixture.paths, &mut package, Some(blobs)).expect("collected files");
    package
}

#[test]
fn every_export_table_is_valid_and_private_settings_are_excluded() {
    let fixture = fixture();
    seed(&fixture);
    let records = fixture
        .database
        .with_connection(|connection| {
            TransferRepository::new(connection).export(&archive::all_categories())
        })
        .expect("all tables");
    assert_eq!(
        records
            .iter()
            .filter(|record| record.kind == "setting")
            .count(),
        1
    );
    let provider = records
        .iter()
        .find(|record| record.kind == "provider")
        .expect("provider");
    assert_eq!(
        text(provider, "settings").expect("settings"),
        "{\"timeoutSeconds\":45}"
    );
    assert!(!provider.fields.contains_key("secret_id"));
    assert!(!provider.fields.contains_key("account_id"));
    assert!(!records.iter().any(|record| matches!(
        record.kind.as_str(),
        "run" | "secret" | "download" | "access_grant"
    )));
}

#[test]
fn archive_round_trip_restores_files_and_links_without_rewriting_message_text() {
    let source = fixture();
    seed(&source);
    let blobs = source._directory.join("blobs");
    let package = package(&source, vec![Connections, Conversations, Settings], &blobs);
    let archive_path = source._directory.join("sample.dbox");
    archive::write(&package, &blobs, &archive_path).expect("archive written");
    let read_blobs = source._directory.join("read-blobs");
    let (package, hash) = archive::read(&archive_path, &read_blobs).expect("archive read");
    let target = fixture();
    restore::restore(
        &target.paths,
        &target.database,
        3,
        &package,
        &hash,
        &read_blobs,
        &package.categories,
    )
    .expect("restored");
    let (id, provider) = target
        .database
        .with_connection(|connection| {
            let (id, provider): (String, String) =
                connection.query_row("SELECT id,provider_id FROM conversation", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            let (saved, enabled, secret): (String, bool, Option<String>) =
                connection.query_row("SELECT id,enabled,secret_id FROM provider", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            assert_eq!(provider, saved);
            let (default, model, external): (String,String,String) = connection.query_row("SELECT provider.default_model_id,model.id,model.external_id FROM provider JOIN model ON model.provider_id=provider.id", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
            assert_eq!(default, model);
            assert_ne!(model, MODEL);
            assert_eq!(external, MODEL);
            assert!(!enabled);
            assert_eq!(secret, None);
            let content: String =
                connection.query_row("SELECT content FROM message", [], |row| row.get(0))?;
            assert_eq!(content, format!("Literal {PROVIDER}"));
            Ok((id, provider))
        })
        .expect("links");
    assert_ne!(id, CONVERSATION);
    assert_ne!(provider, PROVIDER);
    assert_eq!(
        fs::read(
            target
                .paths
                .conversations_dir
                .join(id)
                .join("attachments/sample.txt")
        )
        .expect("copied file"),
        b"attachment"
    );
    let duplicate = restore::restore(
        &target.paths,
        &target.database,
        4,
        &package,
        &hash,
        &read_blobs,
        &package.categories,
    )
    .expect_err("duplicate refused");
    assert_eq!(duplicate.code, AppErrorCode::Conflict);
    assert_eq!(duplicate.details.expect("reason")["reason"], "duplicate");
}

#[test]
fn compactions_round_trip_plain_text_and_json_like_summaries_without_rewriting_them() {
    use crate::data::repositories::ChatRepository;
    use crate::data::schema::CompactionRow;
    use zvs_contract::CompactionTrigger;

    let source = fixture();
    seed(&source);
    let originals: Vec<_> = [
        (
            CompactionTrigger::Manual,
            format!("Summary for {PROVIDER}\nPlain text"),
        ),
        (
            CompactionTrigger::Auto,
            json!({ "providerId": PROVIDER }).to_string(),
        ),
        (CompactionTrigger::Manual, "Historical summary".to_owned()),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (trigger, summary))| CompactionRow {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: CONVERSATION.to_owned(),
        boundary_message_id: MESSAGE.to_owned(),
        summary,
        messages: 1,
        tokens_before: 100,
        tokens_after: 20,
        provider_id: if index == 2 {
            REMOVED_PROVIDER
        } else {
            PROVIDER
        }
        .to_owned(),
        model_id: "fake-model".to_owned(),
        trigger,
        created_at: index as i64 + 1,
    })
    .collect();
    source
        .database
        .with_connection(|connection| {
            for row in &originals {
                ChatRepository::new(connection).add_compaction(row)?;
            }
            Ok(())
        })
        .expect("compactions stored");
    let blobs = source._directory.join("blobs");
    let package = package(&source, vec![Connections, Conversations], &blobs);
    let archive_path = source._directory.join("compactions.dbox");
    archive::write(&package, &blobs, &archive_path).expect("compactions exported");
    let read_blobs = source._directory.join("read-blobs");
    let (package, hash) = archive::read(&archive_path, &read_blobs).expect("archive read");
    let target = fixture();
    restore::restore(
        &target.paths,
        &target.database,
        3,
        &package,
        &hash,
        &read_blobs,
        &package.categories,
    )
    .expect("compactions restored");
    target
        .database
        .with_connection(|connection| {
            let (conversation, provider): (String, String) =
                connection.query_row("SELECT id,provider_id FROM conversation", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            let message: String =
                connection.query_row("SELECT id FROM message", [], |row| row.get(0))?;
            let restored = ChatRepository::new(connection).compactions(&conversation)?;
            assert_eq!(restored.len(), originals.len());
            for (restored, original) in restored.iter().zip(&originals) {
                assert_ne!(restored.id, original.id);
                assert_eq!(restored.conversation_id, conversation);
                assert_eq!(restored.boundary_message_id, message);
                assert_eq!(
                    restored.provider_id,
                    if original.provider_id == PROVIDER {
                        &provider
                    } else {
                        &original.provider_id
                    }
                    .as_str(),
                );
                assert_eq!(restored.summary, original.summary);
                assert_eq!(restored.trigger, original.trigger);
                assert_eq!(restored.model_id, original.model_id);
                assert_eq!(restored.messages, original.messages);
                assert_eq!(restored.tokens_before, original.tokens_before);
                assert_eq!(restored.tokens_after, original.tokens_after);
            }
            Ok(())
        })
        .expect("compaction contents and references");
}

#[test]
fn dialogs_with_removed_providers_import_without_blocking_preview_or_empty_dependencies() {
    let source = fixture();
    seed(&source);
    source
        .database
        .with_connection(|connection| {
            connection.execute("DELETE FROM provider WHERE id=?1", [PROVIDER])?;
            Ok(())
        })
        .expect("provider removed while retaining history");
    let blobs = source._directory.join("blobs");
    let package = package(&source, vec![Connections, Conversations], &blobs);
    let archive_path = source._directory.join("removed-provider.dbox");
    archive::write(&package, &blobs, &archive_path).expect("history exported");
    let read_blobs = source._directory.join("read-blobs");
    let (package, hash) = archive::read(&archive_path, &read_blobs).expect("archive read");
    let target = fixture();
    let preview = import_preview(&target.database, &package, &hash).expect("preview");
    assert!(preview.blocked_categories.is_empty());
    let dialogs = preview
        .categories
        .iter()
        .find(|category| category.category == Conversations)
        .expect("dialogs");
    assert_eq!(dialogs.count, 1);
    assert!(dialogs.requires.is_empty());
    restore::restore(
        &target.paths,
        &target.database,
        3,
        &package,
        &hash,
        &read_blobs,
        &[Conversations],
    )
    .expect("history imported without a provider configuration");
    target
        .database
        .with_connection(|connection| {
            let (id, provider): (String, String) =
                connection.query_row("SELECT id,provider_id FROM conversation", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            assert_ne!(id, CONVERSATION);
            assert_eq!(provider, PROVIDER);
            let message_owner: String =
                connection
                    .query_row("SELECT conversation_id FROM message", [], |row| row.get(0))?;
            assert_eq!(message_owner, id);
            assert_eq!(
                connection.query_row("SELECT count(*) FROM provider", [], |row| row
                    .get::<_, i64>(0))?,
                0
            );
            let store = Record {
                kind: "vector_store".to_owned(),
                fields: [("embedding_provider_id".to_owned(), json!(PROVIDER))].into(),
            };
            assert_eq!(
                TransferRepository::new(connection)
                    .blocked_categories(&[store])
                    .expect("dependencies"),
                vec![zvs_contract::TransferCategory::Stores]
            );
            Ok(())
        })
        .expect("history preserved without creating a provider");
}

#[test]
fn structured_scenario_fields_still_reject_plain_text() {
    use crate::data::repositories::data_transfer::validate_fields;

    for (kind, field) in [("scenario", "trigger"), ("scenario_version", "summary")] {
        let mut record = Record {
            kind: kind.to_owned(),
            fields: Default::default(),
        };
        set(&mut record, field, "plain text");
        assert_eq!(
            validate_fields(&record).expect_err("JSON required").code,
            AppErrorCode::UnsupportedFormat
        );
        set(&mut record, field, "{}");
        validate_fields(&record).expect("valid JSON");
    }
}

#[test]
fn failed_import_rolls_back_records_and_removes_published_files() {
    let source = fixture();
    seed(&source);
    let blobs = source._directory.join("blobs");
    let mut package = package(&source, vec![Connections, Conversations, Memory], &blobs);
    let mut invalid = Record {
        kind: "memory".to_owned(),
        fields: std::collections::BTreeMap::new(),
    };
    for (key, value) in [
        ("id", json!(uuid::Uuid::new_v4().to_string())),
        ("scope", json!("invalid")),
        ("conversation_id", json!(null)),
        ("content", json!("test")),
        ("source", json!("user")),
        ("created_at", json!(1)),
        ("updated_at", json!(1)),
    ] {
        set(&mut invalid, key, value);
    }
    package.records.push(invalid);
    let target = fixture();
    assert!(
        restore::restore(
            &target.paths,
            &target.database,
            3,
            &package,
            "test",
            &blobs,
            &package.categories
        )
        .is_err()
    );
    target
        .database
        .with_connection(|connection| {
            assert_eq!(
                connection.query_row("SELECT count(*) FROM provider", [], |row| row
                    .get::<_, i64>(0))?,
                0
            );
            assert_eq!(
                connection.query_row("SELECT count(*) FROM conversation", [], |row| row
                    .get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .expect("rolled back");
    assert_eq!(
        fs::read_dir(&target.paths.conversations_dir)
            .expect("conversations")
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(target.paths.transfer_journals_dir)
            .expect("journals")
            .count(),
        0
    );
}

#[test]
fn unsafe_members_unknown_records_and_newer_formats_are_rejected() {
    for path in [
        "../outside",
        "/outside",
        "a/../b",
        "C:/outside",
        "a\\b",
        "NUL.txt",
        "file:stream",
        "a./b",
        "a//b",
    ] {
        assert!(!archive::safe_relative(path), "{path}");
    }
    let source = fixture();
    let mut package = package(&source, vec![Settings], &source._directory.join("blobs"));
    package.version = 2;
    assert!(package.validate().is_err());
    package.version = 1;
    package.records.push(Record {
        kind: "access_grant".to_owned(),
        fields: Default::default(),
    });
    assert!(package.validate().is_err());
}

#[test]
fn truncated_archives_and_blob_corruption_are_rejected() {
    let source = fixture();
    seed(&source);
    let blobs = source._directory.join("blobs");
    let package = package(&source, vec![Connections, Conversations], &blobs);
    let path = source._directory.join("archive.dbox");
    archive::write(&package, &blobs, &path).expect("archive");
    let bytes = fs::read(&path).expect("bytes");
    fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate");
    assert!(archive::read(&path, &source._directory.join("read")).is_err());
    fs::write(blobs.join(&package.files[0].hash), b"corruption").expect("corrupt blob");
    archive::write(&package, &blobs, &path).expect("archive");
    assert!(archive::read(&path, &source._directory.join("read2")).is_err());
}

#[tokio::test]
async fn uploads_require_contiguous_offsets_complete_content_and_explicit_inspection() {
    let fixture = fixture();
    let service = DataTransferService::new(
        fixture.paths.clone(),
        fixture.database.shared(),
        Arc::new(FakeClock::default()),
    )
    .expect("service");
    let session = service
        .begin(TransferUploadInput {
            name: "test.dbox".to_owned(),
            bytes: 4,
        })
        .expect("session");
    let chunk = |offset| TransferChunkInput {
        id: session.id.clone(),
        offset,
        data: "dGVzdA==".to_owned(),
    };
    assert!(service.append(chunk(1)).is_err());
    assert!(service.inspect(session.clone()).await.is_err());
    assert!(
        service
            .import(TransferImportInput {
                id: session.id.clone(),
                categories: vec![Settings]
            })
            .await
            .is_err()
    );
    service.append(chunk(0)).expect("chunk");
    assert!(service.append(chunk(0)).is_err());
    assert!(service.inspect(session.clone()).await.is_err());
    service.dispose(session.clone());
    assert!(service.append(chunk(0)).is_err());
}

#[test]
fn startup_recovery_removes_uncommitted_files_and_preserves_committed_files() {
    let fixture = fixture();
    let batch = uuid::Uuid::new_v4().to_string();
    let owner = uuid::Uuid::new_v4().to_string();
    let target = fixture.paths.conversations_dir.join(&owner);
    fs::create_dir_all(&target).expect("target");
    fs::write(target.join("test.txt"), "test").expect("file");
    let directory = fixture.paths.transfer_journals_dir.clone();
    fs::create_dir_all(&directory).expect("journals");
    let journal = directory.join(format!("{batch}.json"));
    fs::write(
        &journal,
        json!({"batch":batch,"conversations":[owner],"skills":[]}).to_string(),
    )
    .expect("journal");
    restore::recover(&fixture.paths, &fixture.database).expect("recover");
    assert!(!target.exists());
    fs::create_dir_all(&target).expect("target");
    fs::write(
        &journal,
        json!({"batch":batch,"conversations":[owner],"skills":[]}).to_string(),
    )
    .expect("journal");
    fixture
        .database
        .with_connection(|connection| {
            TransferRepository::new(connection).mark(&format!("transfer.committed.{batch}"), 1)
        })
        .expect("committed");
    restore::recover(&fixture.paths, &fixture.database).expect("recover");
    assert!(target.exists());
    assert!(!journal.exists());
}

#[test]
fn all_categories_restore_disabled_configuration_and_reindexable_document_copies() {
    let source = fixture();
    seed(&source);
    let skill = uuid::Uuid::new_v4().to_string();
    let scenario = uuid::Uuid::new_v4().to_string();
    let version = uuid::Uuid::new_v4().to_string();
    let store = uuid::Uuid::new_v4().to_string();
    let document = uuid::Uuid::new_v4().to_string();
    let skill_path = source.paths.skills_dir.join("sample");
    fs::create_dir_all(&skill_path).expect("skill folder");
    fs::write(
        skill_path.join("SKILL.md"),
        "---\nname: Sample\ndescription: Test\n---\nTest",
    )
    .expect("skill");
    let document_path = source._directory.join("document.txt");
    fs::write(&document_path, "Document text").expect("document");
    let summary: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/shared/fixtures/scenarios/ScenarioDiffDto.valid.json"
    )))
    .expect("summary fixture");
    let graph = json!({ "nodes": [{ "id": PROVIDER, "type": "model.chat", "position": {"x":0,"y":0}, "params": {"providerId":PROVIDER,"modelId":MODEL,"tools":["local.call"]} }], "edges":[], "attachments":[] });
    source.database.with_connection(|connection| {
        connection.execute("INSERT INTO mcp_server (id,name,namespace,transport,command,created_at,updated_at) VALUES (?1,'Connector','local','stdio','node',1,2)", [uuid::Uuid::new_v4().to_string()])?;
        connection.execute("INSERT INTO integration_account (id,kind,name,config,created_at,updated_at) VALUES (?1,'telegram','Telegram','{}',1,2)", [uuid::Uuid::new_v4().to_string()])?;
        connection.execute("INSERT INTO skill (id,key,name,description,content_hash,path,file_count,created_at,updated_at) VALUES (?1,'sample','Sample','Test',?2,?3,1,1,2)", rusqlite::params![skill, "0".repeat(64), source.paths.stored_paths().encode(&skill_path)])?;
        connection.execute("INSERT INTO scenario (id,name,active,current_version_id,created_at,updated_at) VALUES (?1,'Scenario',1,?2,1,2)", rusqlite::params![scenario, version])?;
        connection.execute("INSERT INTO scenario_version (id,scenario_id,version,graph,summary,created_at) VALUES (?1,?2,1,?3,?4,1)", rusqlite::params![version,scenario,graph.to_string(),summary[0]["value"].to_string()])?;
        connection.execute("INSERT INTO memory (id,scope,conversation_id,content,source,created_at,updated_at) VALUES (?1,'conversation',?2,'Memory','user',1,2)", rusqlite::params![uuid::Uuid::new_v4().to_string(),CONVERSATION])?;
        connection.execute("INSERT INTO vector_store (id,name,embedding_provider_id,embedding_model_id,dimension,metric,chunk_size,chunk_overlap,created_at,updated_at) VALUES (?1,'Knowledge',?2,'embedding',3,'cosine',512,64,1,2)", rusqlite::params![store,PROVIDER])?;
        connection.execute("INSERT INTO vector_document (id,store_id,source_path,content_hash,chunk_count,bytes,indexed_at) VALUES (?1,?2,?3,?4,1,13,1)", rusqlite::params![document,store,document_path.to_string_lossy(),"0".repeat(64)])?;
        connection.execute("UPDATE conversation SET attached_store_ids=?1", [json!([store]).to_string()])?;
        Ok(())
    }).expect("all categories seeded");
    source
        .database
        .with_connection(|connection| {
            connection.execute(
                "UPDATE mcp_server SET env=?1",
                [json!({"TOKEN":{"kind":"secret","secretId":PROVIDER}}).to_string()],
            )?;
            connection.execute(
                "UPDATE integration_account SET config=?1",
                [json!({"token":{"secretId":PROVIDER}}).to_string()],
            )?;
            Ok(())
        })
        .expect("credential bindings");
    let blobs = source._directory.join("blobs");
    let package = package(&source, archive::all_categories(), &blobs);
    for record in &package.records {
        if record.kind == "mcp_server" || record.kind == "integration_account" {
            assert!(
                !serde_json::to_string(record)
                    .expect("configuration")
                    .contains("secretId")
            );
        }
    }
    let target = fixture();
    target.database.with_connection(|connection| { connection.execute("INSERT INTO mcp_server (id,name,namespace,transport,command,enabled,created_at,updated_at) VALUES (?1,'Connector','local','stdio','node',0,1,2)", [uuid::Uuid::new_v4().to_string()])?; Ok(()) }).expect("existing connector");
    restore::restore(
        &target.paths,
        &target.database,
        3,
        &package,
        "all",
        &blobs,
        &package.categories,
    )
    .expect("all categories restored");
    target
        .database
        .with_connection(|connection| {
            for (kind, flag) in [
                ("provider", "enabled"),
                ("mcp_server", "enabled"),
                ("integration_account", "enabled"),
                ("skill", "enabled"),
                ("scenario", "active"),
            ] {
                let active: i64 = connection.query_row(
                    &format!("SELECT sum({flag}) FROM {kind}"),
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(active, 0, "{kind}");
            }
            let provider: String =
                connection.query_row("SELECT id FROM provider", [], |row| row.get(0))?;
            let graph: String =
                connection.query_row("SELECT graph FROM scenario_version", [], |row| row.get(0))?;
            let graph: Value = serde_json::from_str(&graph).expect("graph");
            assert_eq!(graph["nodes"][0]["params"]["providerId"], provider);
            assert_eq!(graph["nodes"][0]["id"], PROVIDER);
            assert_eq!(graph["nodes"][0]["params"]["modelId"], MODEL);
            assert_eq!(graph["nodes"][0]["params"]["tools"][0], "local2.call");
            let document: String =
                connection.query_row("SELECT path FROM vector_source", [], |row| row.get(0))?;
            assert_eq!(
                fs::read_to_string(document).expect("document copy"),
                "Document text"
            );
            assert_eq!(
                connection.query_row("SELECT count(*) FROM vector_document", [], |row| row
                    .get::<_, i64>(0))?,
                1
            );
            let conversation: String =
                connection.query_row("SELECT id FROM conversation", [], |row| row.get(0))?;
            let memory: String =
                connection.query_row("SELECT conversation_id FROM memory", [], |row| row.get(0))?;
            assert_eq!(conversation, memory);
            Ok(())
        })
        .expect("restored data");
}

#[test]
fn import_cannot_exceed_memory_limits_or_attach_messages_to_an_existing_dialog() {
    let source = fixture();
    seed(&source);
    source.database.with_connection(|connection| {
        connection.execute("INSERT INTO memory (id,scope,content,source,created_at,updated_at) VALUES (?1,'account','New fact','user',1,2)", [uuid::Uuid::new_v4().to_string()])?;
        Ok(())
    }).expect("memory");
    let target = fixture();
    seed(&target);
    target.database.with_connection(|connection| {
        for index in 0..zvs_contract::MEMORY_ACCOUNT_MAX {
            connection.execute("INSERT INTO memory (id,scope,content,source,created_at,updated_at) VALUES (?1,'account',?2,'user',1,2)", rusqlite::params![uuid::Uuid::new_v4().to_string(),format!("Fact {index}")])?;
        }
        Ok(())
    }).expect("existing memories");
    let blobs = source._directory.join("blobs");
    let memory = package(&source, vec![Memory], &blobs);
    let error = restore::restore(
        &target.paths,
        &target.database,
        3,
        &memory,
        "memory",
        &blobs,
        &[Memory],
    )
    .expect_err("memory limit");
    assert_eq!(error.details.expect("reason")["reason"], "memoryLimit");
    let mut messages = package(&source, vec![Connections, Conversations], &blobs);
    messages
        .records
        .retain(|record| record.kind != "conversation");
    messages.files.clear();
    let error = restore::restore(
        &target.paths,
        &target.database,
        3,
        &messages,
        "messages",
        &blobs,
        &messages.categories,
    )
    .expect_err("foreign dialog");
    assert_eq!(error.details.expect("reason")["reason"], "dependencies");
    target
        .database
        .with_connection(|connection| {
            assert_eq!(
                connection.query_row("SELECT count(*) FROM memory", [], |row| row
                    .get::<_, i64>(0))?,
                zvs_contract::MEMORY_ACCOUNT_MAX as i64
            );
            assert_eq!(
                connection.query_row("SELECT count(*) FROM message", [], |row| row
                    .get::<_, i64>(0))?,
                1
            );
            Ok(())
        })
        .expect("existing records unchanged");
    let message = messages
        .records
        .iter_mut()
        .find(|record| record.kind == "message")
        .expect("message");
    set(message, "tokens_in", -1);
    assert!(messages.validate().is_err());
}

#[test]
fn categories_from_one_archive_can_be_imported_in_separate_steps() {
    let source = fixture();
    seed(&source);
    source.database.with_connection(|connection| {
        connection.execute("INSERT INTO memory (id,scope,conversation_id,content,source,created_at,updated_at) VALUES (?1,'conversation',?2,'Fact','user',1,2)", rusqlite::params![uuid::Uuid::new_v4().to_string(),CONVERSATION])?;
        Ok(())
    }).expect("memory");
    let blobs = source._directory.join("blobs");
    let package = package(&source, vec![Connections, Conversations, Memory], &blobs);
    let target = fixture();
    restore::restore(
        &target.paths,
        &target.database,
        3,
        &package,
        "staged",
        &blobs,
        &[Connections],
    )
    .expect("providers");
    let preview = import_preview(&target.database, &package, "staged").expect("preview");
    assert_eq!(preview.imported_categories, vec![Connections]);
    restore::restore(
        &target.paths,
        &target.database,
        4,
        &package,
        "staged",
        &blobs,
        &[Conversations],
    )
    .expect("conversations");
    restore::restore(
        &target.paths,
        &target.database,
        5,
        &package,
        "staged",
        &blobs,
        &[Memory],
    )
    .expect("memory");
    target
        .database
        .with_connection(|connection| {
            let (conversation, provider): (String, String) =
                connection.query_row("SELECT id,provider_id FROM conversation", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            let saved_provider: String =
                connection.query_row("SELECT id FROM provider", [], |row| row.get(0))?;
            let owner: String =
                connection.query_row("SELECT conversation_id FROM memory", [], |row| row.get(0))?;
            assert_eq!(provider, saved_provider);
            assert_eq!(conversation, owner);
            assert_eq!(
                fs::read(
                    target
                        .paths
                        .conversations_dir
                        .join(conversation)
                        .join("attachments/sample.txt")
                )
                .expect("attachment"),
                b"attachment"
            );
            Ok(())
        })
        .expect("links preserved");
}
