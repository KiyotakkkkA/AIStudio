use std::collections::VecDeque;
use std::fs;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, CreateSkillInput, GraphNode, ProviderCapability, ProviderKind,
    RetryPolicy, RunGraph, RunKind, SKILL_TRIGGER_KEY, SkillDto, SkillExportInput,
    SkillFileEncoding, SkillFileInput, SkillImportInput, SkillIssueDto, SkillIssueRule,
    SkillSource, SkillStatus, SkillUpdateDto, StartRunInput, ToolSource, ToolTier,
    UpdateSkillInput, Validate,
};

use super::bundled::is_newer;
use super::library::{SKILL_BUNDLED_PROTECTED, SKILL_FILE_NOT_FOUND, SKILL_UP_TO_DATE};
use super::manifest::SKILL_FILE;
use super::*;
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::data::schema::ProviderRow;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::{EmbeddingDriver, SharedTextDriver};
use crate::drivers::ai::{
    AiDriver, DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult, TextDelta,
    TextGenerationDriver,
};
use crate::kernel::{
    FnNode, KernelServices, NodeRegistry, PermissionRequirement, StepContext, TextDrivers,
    ToolStepRequest,
};
use crate::platform::events::EventBus;
use crate::services::providers::DriverSource;
use crate::services::runs::{OUTSIDE_ALLOWED_TOOLS, RunServiceOptions};
use crate::services::{RunService, SettingService};
use crate::testing::{
    FAKE_CAPABILITIES, FakePicker, TemporaryDatabase, TemporaryDirectory, temporary_database,
    temporary_directory,
};
use crate::tools::{ToolDescriptor, ToolRegistry, ToolRegistryOptions, register_kernel_tools};

#[derive(Default)]
struct Scripted {
    rounds: Mutex<VecDeque<Vec<TextDelta>>>,
    requests: Mutex<Vec<GenerateRequest>>,
}

#[async_trait]
impl TextGenerationDriver for Scripted {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn generate(
        &self,
        _request: &GenerateRequest,
        _cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        Err(AppError::new(AppErrorCode::Unknown, "not used"))
    }

    fn stream(&self, request: &GenerateRequest, _cancel: &CancellationToken) -> DeltaStream {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        let deltas = self
            .rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| vec![TextDelta::text("Готово.")]);
        Box::pin(stream::iter(deltas.into_iter().map(Ok)))
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct OneDriver(SharedTextDriver);

impl TextDrivers for OneDriver {
    fn text(&self, _provider_id: &str) -> Result<SharedTextDriver, AppError> {
        Ok(Arc::clone(&self.0))
    }
}

struct TopicEmbeddings;

#[async_trait]
impl EmbeddingDriver for TopicEmbeddings {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        _model: &str,
        _cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        Ok(texts
            .iter()
            .map(|text| {
                let text = text.to_lowercase();
                if text.contains("выпуск") {
                    vec![1.0, 0.1]
                } else if text.contains("погод") {
                    vec![0.0, 1.0]
                } else {
                    vec![0.6, 0.6]
                }
            })
            .collect())
    }

    fn dimensions(&self) -> Option<usize> {
        Some(2)
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct TopicSource;

#[async_trait]
impl DriverSource for TopicSource {
    async fn ephemeral_driver(&self, _row: &ProviderRow) -> Result<AiDriver, AppError> {
        Ok(AiDriver {
            embedding: Some(Arc::new(TopicEmbeddings)),
            ..AiDriver::default()
        })
    }

    fn invalidate(&self, _provider_id: &str) {}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
struct LookupInput {
    query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
struct TextOutput {
    text: String,
}

impl Validate for LookupInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

impl Validate for TextOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Default)]
struct Calls {
    lookups: Mutex<Vec<String>>,
    drops: Mutex<usize>,
}

struct Host {
    database: TemporaryDatabase,
    workspace: TemporaryDirectory,
    tools: ToolRegistry,
    runs: RunService,
    nodes: Arc<NodeRegistry>,
    service: SkillService,
    picker: Arc<FakePicker>,
}

fn register_demo_nodes(nodes: &NodeRegistry, calls: &Arc<Calls>) {
    let lookups = Arc::clone(calls);
    nodes
        .register(FnNode::new(
            "demo.lookup",
            PermissionRequirement::tool("demo.lookup"),
            false,
            move |_context: StepContext, input: LookupInput| {
                let calls = Arc::clone(&lookups);
                async move {
                    calls
                        .lookups
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(input.query.clone());
                    Ok(TextOutput {
                        text: format!("найдено: {}", input.query),
                    })
                }
            },
        ))
        .expect("registers lookup");
    let drops = Arc::clone(calls);
    nodes
        .register(FnNode::new(
            "demo.drop",
            PermissionRequirement::tool("demo.drop"),
            false,
            move |_context: StepContext, _input: Value| {
                let calls = Arc::clone(&drops);
                async move {
                    *calls.drops.lock().unwrap_or_else(PoisonError::into_inner) += 1;
                    Ok(TextOutput {
                        text: "удалено".to_owned(),
                    })
                }
            },
        ))
        .expect("registers drop");
    nodes
        .register(FnNode::new(
            "demo.probe",
            PermissionRequirement::None,
            false,
            move |context: StepContext, input: Value| async move {
                let key = input["key"].as_str().unwrap_or_default().to_owned();
                let invocation = context
                    .invoke_tool(ToolStepRequest {
                        key,
                        input: json!({ "query": "x" }),
                        label: "probe".to_owned(),
                        minimum_tier: None,
                        on_approval: None,
                    })
                    .expect("the kernel offers tool calls");
                match invocation.await {
                    Ok(_) => Ok(json!({ "admitted": true })),
                    Err(error) => Ok(
                        json!({ "admitted": false, "code": error.code, "message": error.message }),
                    ),
                }
            },
        ))
        .expect("registers probe");
}

fn host() -> Host {
    host_with(None)
}

fn host_with(drivers: Option<Arc<dyn DriverSource>>) -> Host {
    let database = temporary_database();
    let workspace = temporary_directory("studio-skills-");
    let nodes = Arc::new(NodeRegistry::new());
    let calls = Arc::new(Calls::default());
    register_demo_nodes(&nodes, &calls);
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    let register = |key: &str| {
        tools
            .register(&ToolDescriptor {
                description: format!("Инструмент {key}"),
                input_schema: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
                read_only: true,
                ..ToolDescriptor::new(key, ToolSource::Builtin)
            })
            .expect("registers a tool")
    };
    register("demo.lookup");
    register("demo.drop");
    let library = SkillLibrary::new(SkillLibraryOptions::new(
        database.shared(),
        workspace.join(SKILLS_DIR),
    ));
    register_skill_nodes(&nodes, &library).expect("registers the skill nodes");
    register_kernel_tools(&nodes, &tools).expect("registers the kernel tools");
    let driver = Arc::new(Scripted::default());
    let runs = RunService::new(RunServiceOptions {
        services: KernelServices {
            providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
            ..KernelServices::default()
        },
        tools: Some(tools.clone()),
        ..RunServiceOptions::new(database.shared(), EventBus::default(), Arc::clone(&nodes))
    })
    .expect("a run service");
    database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let provider = repositories.providers.create(&ProviderDraft::new(
                ProviderKind::Ollama,
                "Текст",
                "http://localhost:11434",
                vec![ProviderCapability::Text],
                1,
            ))?;
            repositories.models.replace_for_provider(
                &provider.id,
                &[ModelDraft {
                    context_window: Some(16384),
                    max_output: Some(512),
                    ..ModelDraft::new("fake-model", "Fake", 1)
                }],
                1,
            )?;
            Ok(())
        })
        .expect("seeds a provider");
    let picker = FakePicker::shared();
    let settings = SettingService::new(
        database.shared(),
        Arc::new(crate::platform::clock::FakeClock::default()),
    );
    let service = SkillService::new(SkillServiceOptions {
        drivers,
        settings: Some(settings),
        picker: Some(picker.clone()),
        ..SkillServiceOptions::new(library, database.shared())
    });
    Host {
        database,
        workspace,
        tools,
        runs,
        nodes,
        service,
        picker,
    }
}

fn notes() -> CreateSkillInput {
    CreateSkillInput {
        key: "release-notes".to_owned(),
        name: "Заметки о выпуске".to_owned(),
        description: "Составляет заметки о выпуске по списку изменений".to_owned(),
        instructions: "# Заметки\n\nСгруппируйте изменения по разделам.".to_owned(),
        enabled: None,
        files: Some(vec![
            SkillFileInput {
                path: "references/style.md".to_owned(),
                content: "Пишите кратко.".to_owned(),
                encoding: None,
            },
            SkillFileInput {
                path: "assets/logo.bin".to_owned(),
                content: "AAEC".to_owned(),
                encoding: Some(SkillFileEncoding::Base64),
            },
        ]),
    }
}

fn weather() -> CreateSkillInput {
    CreateSkillInput {
        key: "weather".to_owned(),
        name: "Погода".to_owned(),
        description: "Рассказывает прогноз погоды в городе".to_owned(),
        instructions: "Назовите температуру.".to_owned(),
        enabled: None,
        files: None,
    }
}

fn issues_of(error: &AppError) -> Vec<SkillIssueDto> {
    serde_json::from_value(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("issues"))
            .cloned()
            .unwrap_or(Value::Null),
    )
    .expect("the error lists its issues")
}

fn issue(field: &str, rule: SkillIssueRule) -> SkillIssueDto {
    SkillIssueDto {
        field: field.to_owned(),
        rule,
    }
}

async fn finished(host: &Host, handle: &zvs_contract::RunHandleDto) -> zvs_contract::RunDto {
    tokio::time::timeout(Duration::from_secs(10), host.runs.wait(&handle.id))
        .await
        .expect("the run finishes in time");
    host.runs.get(&handle.id).expect("the run")
}

#[test]
fn a_skill_is_saved_as_a_folder_with_its_manifest_and_files_and_indexed_in_the_database() {
    let host = host();
    let detail = host.service.create(&notes()).expect("creates the skill");
    assert_eq!(detail.skill.status, SkillStatus::Ready);
    assert_eq!(detail.skill.source, SkillSource::Local);
    assert_eq!(detail.skill.file_count, 2);
    assert_eq!(
        detail.instructions,
        "# Заметки\n\nСгруппируйте изменения по разделам."
    );
    assert_eq!(
        detail
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["assets/logo.bin", "references/style.md"]
    );
    let folder = host.workspace.join(SKILLS_DIR).join("release-notes");
    let manifest = fs::read_to_string(folder.join(SKILL_FILE)).expect("a manifest");
    assert!(manifest.starts_with("---\nname: \"release-notes\"\n"));
    assert!(!manifest.contains("allowed-tools"));
    assert_eq!(
        fs::read(folder.join("assets").join("logo.bin")).expect("an asset"),
        [0, 1, 2]
    );
    let entries: Vec<String> = fs::read_dir(host.workspace.join(SKILLS_DIR))
        .expect("the skills folder")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(entries, ["release-notes"]);
}

fn update(id: zvs_contract::SkillId) -> UpdateSkillInput {
    UpdateSkillInput {
        id,
        key: None,
        name: None,
        description: None,
        instructions: None,
        enabled: None,
        files: None,
        remove_files: None,
    }
}

#[test]
fn every_field_problem_is_reported_at_once_for_the_editor() {
    let host = host();
    host.service.create(&weather()).expect("creates weather");
    let error = host
        .service
        .create(&CreateSkillInput {
            key: "weather".to_owned(),
            name: " ".to_owned(),
            description: String::new(),
            instructions: "\n".to_owned(),
            enabled: None,
            files: Some(vec![
                SkillFileInput {
                    path: "../escape.md".to_owned(),
                    content: "x".to_owned(),
                    encoding: None,
                },
                SkillFileInput {
                    path: "assets/a.bin".to_owned(),
                    content: "not base64!".to_owned(),
                    encoding: Some(SkillFileEncoding::Base64),
                },
            ]),
        })
        .expect_err("rejected");
    assert_eq!(
        issues_of(&error),
        [
            issue("files.0", SkillIssueRule::Path),
            issue("files.1", SkillIssueRule::Encoding),
            issue("key", SkillIssueRule::Duplicate),
            issue("name", SkillIssueRule::Required),
            issue("description", SkillIssueRule::Required),
            issue("instructions", SkillIssueRule::Required),
        ]
    );
    let details = error.details.expect("details");
    assert_eq!(details["field"], json!("files.0"));
    assert_eq!(details["rule"], json!("path"));
    let pattern = host
        .service
        .create(&CreateSkillInput {
            key: "Weather Now".to_owned(),
            ..weather()
        })
        .expect_err("rejected");
    assert_eq!(issues_of(&pattern), [issue("key", SkillIssueRule::Pattern)]);
}

#[test]
fn an_update_renames_the_folder_replaces_files_and_keeps_the_rest() {
    let host = host();
    let saved = host.service.create(&notes()).expect("creates");
    let updated = host
        .service
        .update(&UpdateSkillInput {
            key: Some("changelog".to_owned()),
            instructions: Some("Новые шаги.".to_owned()),
            files: Some(vec![SkillFileInput {
                path: "scripts/build.py".to_owned(),
                content: "print('ok')".to_owned(),
                encoding: None,
            }]),
            remove_files: Some(vec!["assets/logo.bin".to_owned()]),
            ..update(saved.skill.id)
        })
        .expect("updates");
    assert_eq!(updated.skill.key, "changelog");
    assert_eq!(updated.skill.name, "Заметки о выпуске");
    assert_eq!(updated.instructions, "Новые шаги.");
    assert_eq!(
        updated
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["references/style.md", "scripts/build.py"]
    );
    let root = host.workspace.join(SKILLS_DIR);
    assert!(!root.join("release-notes").exists());
    assert!(
        root.join("changelog")
            .join("references")
            .join("style.md")
            .is_file()
    );
    let missing = host
        .service
        .update(&UpdateSkillInput {
            remove_files: Some(vec!["assets/none.png".to_owned()]),
            ..update(saved.skill.id)
        })
        .expect_err("removing a file that is not there is reported");
    assert_eq!(
        issues_of(&missing),
        [issue("removeFiles.0", SkillIssueRule::Missing)]
    );
}

#[test]
fn a_hand_edit_is_re_read_and_a_broken_skill_is_reported_invalid() {
    let host = host();
    let saved = host.service.create(&notes()).expect("creates");
    let manifest = host
        .workspace
        .join(SKILLS_DIR)
        .join("release-notes")
        .join(SKILL_FILE);
    fs::write(
        &manifest,
        "---\nname: release-notes\ndescription: Отредактировано вручную\nallowed-tools: demo.lookup\n---\n\nНовая инструкция.\n",
    )
    .expect("edits by hand");
    let edited = host.service.get(&saved.skill.id).expect("re-reads");
    assert_eq!(edited.skill.description, "Отредактировано вручную");
    assert_eq!(edited.instructions, "Новая инструкция.");
    assert_eq!(edited.skill.status, SkillStatus::Ready);
    fs::write(&manifest, "no frontmatter").expect("breaks the manifest");
    let broken = host.service.get(&saved.skill.id).expect("re-reads");
    assert_eq!(broken.skill.status, SkillStatus::Invalid);
    assert!(
        broken
            .skill
            .issues
            .contains(&issue("instructions", SkillIssueRule::Manifest))
    );
    assert!(host.service.library().usable().expect("lists").is_empty());
}

#[test]
fn a_folder_dropped_into_the_skills_directory_is_adopted_on_reconcile() {
    let host = host();
    let folder = host.workspace.join(SKILLS_DIR).join("dropped");
    fs::create_dir_all(folder.join("references")).expect("creates a folder");
    fs::write(
        folder.join(SKILL_FILE),
        "---\nname: dropped\ndescription: Скопирован вручную\nmetadata:\n  display-name: Скопированный\n---\n\nШаги.\n",
    )
    .expect("writes a manifest");
    fs::write(folder.join("references").join("a.md"), "a").expect("writes a file");
    fs::create_dir_all(host.workspace.join(SKILLS_DIR).join(".staging-left")).expect("a leftover");
    let skills = host.service.library().reconcile().expect("reconciles");
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].key, "dropped");
    assert_eq!(skills[0].name, "Скопированный");
    assert_eq!(skills[0].status, SkillStatus::Ready);
    assert_eq!(skills[0].file_count, 1);
    assert!(
        !host
            .workspace
            .join(SKILLS_DIR)
            .join(".staging-left")
            .exists()
    );
}

#[tokio::test]
async fn import_and_export_round_trip_a_skill_folder() {
    let host = host();
    let original = host.service.create(&notes()).expect("creates");
    let exports = temporary_directory("studio-skill-export-");
    host.picker.answer(&[&exports.path().to_string_lossy()]);
    let exported = host
        .service
        .export(&SkillExportInput {
            id: original.skill.id,
            path: None,
        })
        .await
        .expect("exports")
        .path
        .expect("a path");
    assert!(std::path::Path::new(&exported).join(SKILL_FILE).is_file());
    let again = host
        .service
        .export(&SkillExportInput {
            id: original.skill.id,
            path: Some(exports.path().to_string_lossy().into_owned()),
        })
        .await
        .expect_err("an existing export is not overwritten");
    assert_eq!(again.code, AppErrorCode::Conflict);
    host.service.remove(&original.skill.id).expect("removes");
    assert!(
        !host
            .workspace
            .join(SKILLS_DIR)
            .join("release-notes")
            .exists()
    );
    let imported = host
        .service
        .import(&SkillImportInput {
            path: Some(exported.clone()),
        })
        .await
        .expect("imports")
        .skill
        .expect("a skill");
    assert_eq!(imported.skill.source, SkillSource::Imported);
    assert_eq!(imported.skill.key, original.skill.key);
    assert_eq!(imported.skill.name, original.skill.name);
    assert_eq!(imported.skill.description, original.skill.description);
    assert_eq!(imported.instructions, original.instructions);
    assert_eq!(imported.files, original.files);
    host.picker.answer(&[]);
    let cancelled = host
        .service
        .import(&SkillImportInput { path: None })
        .await
        .expect("a cancelled picker is not an error");
    assert_eq!(cancelled.skill, None);
}

#[tokio::test]
async fn a_foreign_skill_imports_ready_with_its_tool_list_kept_and_loose_files_moved() {
    let host = host();
    let foreign = temporary_directory("studio-skill-foreign-");
    let folder = foreign.join("pdf-tools");
    fs::create_dir_all(&folder).expect("creates a folder");
    fs::write(
        folder.join(SKILL_FILE),
        "---\nname: pdf-tools\ndescription: Работа с PDF\nallowed-tools: Read Grep\nlicense: MIT\n---\n\nСм. REFERENCE.md.\n",
    )
    .expect("writes a manifest");
    fs::write(folder.join("REFERENCE.md"), "справка").expect("writes a loose file");
    let imported = host
        .service
        .import(&SkillImportInput {
            path: Some(folder.to_string_lossy().into_owned()),
        })
        .await
        .expect("imports")
        .skill
        .expect("a skill");
    assert_eq!(imported.skill.status, SkillStatus::Ready);
    assert!(imported.skill.issues.is_empty());
    assert_eq!(imported.files[0].path, "references/REFERENCE.md");
    let manifest = fs::read_to_string(
        host.workspace
            .join(SKILLS_DIR)
            .join("pdf-tools")
            .join(SKILL_FILE),
    )
    .expect("a manifest");
    assert!(manifest.contains("license: \"MIT\""));
    assert!(manifest.contains("allowed-tools: \"Read Grep\""));
    let empty = temporary_directory("studio-skill-empty-");
    let refused = host
        .service
        .import(&SkillImportInput {
            path: Some(empty.path().to_string_lossy().into_owned()),
        })
        .await
        .expect_err("a folder without SKILL.md is refused");
    assert_eq!(refused.message, library::SKILL_IMPORT_NO_MANIFEST);
}

#[tokio::test]
async fn trigger_scoring_ranks_a_matching_phrase_above_an_unrelated_one() {
    let host = host();
    host.service.create(&notes()).expect("creates notes");
    host.service.create(&weather()).expect("creates weather");
    let cancel = CancellationToken::new();
    let keys = |skills: Vec<SkillDto>| {
        skills
            .into_iter()
            .map(|skill| skill.key)
            .collect::<Vec<_>>()
    };
    let everything = host
        .service
        .select("Прогноз погоды", &cancel)
        .await
        .expect("selects");
    assert_eq!(keys(everything).len(), 2);
    host.database
        .with_connection(|connection| {
            Repositories::new(connection).settings.set(
                SKILL_TRIGGER_KEY,
                &json!({ "threshold": 0.2, "catalogueLimit": 1 }).to_string(),
                1,
            )?;
            Ok(())
        })
        .expect("configures the threshold");
    assert!((host.service.trigger_settings().threshold - 0.2).abs() < f64::EPSILON);
    let weather = host
        .service
        .select("Прогноз погоды", &cancel)
        .await
        .expect("selects");
    assert_eq!(keys(weather), ["weather"]);
    let notes = host
        .service
        .select("Составь заметки к выпуску 2.4", &cancel)
        .await
        .expect("selects");
    assert_eq!(keys(notes), ["release-notes"]);
}

#[tokio::test]
async fn trigger_scoring_uses_the_embedding_provider_when_one_is_configured() {
    let host = host_with(Some(Arc::new(TopicSource)));
    let embedding = host
        .database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let provider = repositories.providers.create(&ProviderDraft::new(
                ProviderKind::Ollama,
                "Эмбеддинги",
                "http://localhost:11435",
                vec![ProviderCapability::Embedding],
                1,
            ))?;
            Ok(provider.id)
        })
        .expect("seeds an embedding provider");
    host.database
        .with_connection(|connection| {
            Repositories::new(connection).settings.set(
                SKILL_TRIGGER_KEY,
                &json!({
                    "threshold": 0.8,
                    "catalogueLimit": 1,
                    "embedding": { "providerId": embedding, "modelId": "embed" }
                })
                .to_string(),
                1,
            )?;
            Ok(())
        })
        .expect("configures embeddings");
    host.service.create(&notes()).expect("creates notes");
    host.service.create(&weather()).expect("creates weather");
    let picked = host
        .service
        .select("Прогноз погоды к выпуску", &CancellationToken::new())
        .await
        .expect("selects");
    assert_eq!(
        picked
            .iter()
            .map(|skill| skill.key.as_str())
            .collect::<Vec<_>>(),
        ["release-notes"]
    );
}

async fn probe_output(host: &Host, handle: zvs_contract::RunHandleDto) -> Value {
    finished(host, &handle).await;
    host.runs
        .steps(&handle.id)
        .expect("steps")
        .into_iter()
        .find(|step| step.node_id == "probe")
        .and_then(|step| step.output)
        .expect("the probe output")
}

#[tokio::test]
async fn the_kernel_refuses_a_model_call_outside_the_runs_allowed_tools() {
    let host = host();
    let probe = |key: &str, allowed: Option<Vec<String>>| StartRunInput {
        kind: RunKind::Skill,
        subject_id: None,
        title: None,
        retry_of_id: None,
        graph: RunGraph {
            nodes: vec![GraphNode {
                dependencies: Some(Vec::new()),
                input: Some(json!({ "key": key })),
                bindings: Some(std::collections::BTreeMap::new()),
                retry: Some(RetryPolicy::default()),
                ..GraphNode::new("probe", "demo.probe")
            }],
            permission_scopes: Some(vec!["skill:x".to_owned()]),
            allowed_tools: allowed,
        },
        input: None,
        concurrency: Some(1),
    };
    let narrowed = Some(vec!["demo.lookup".to_owned()]);
    let refused = probe_output(
        &host,
        host.runs
            .start(probe("demo.drop", narrowed.clone()))
            .expect("starts"),
    )
    .await;
    assert_eq!(refused["admitted"], json!(false));
    assert_eq!(refused["code"], json!("PERMISSION_DENIED"));
    assert_eq!(refused["message"], json!(OUTSIDE_ALLOWED_TOOLS));
    let admitted = probe_output(
        &host,
        host.runs
            .start(probe("demo.lookup", narrowed))
            .expect("starts"),
    )
    .await;
    assert_eq!(admitted["admitted"], json!(true));
    let open = probe_output(
        &host,
        host.runs.start(probe("demo.drop", None)).expect("starts"),
    )
    .await;
    assert_eq!(open["admitted"], json!(true));
}

fn rewrite_metadata(host: &Host, key: &str, metadata: &str) {
    let manifest = host.workspace.join(SKILLS_DIR).join(key).join(SKILL_FILE);
    let text = fs::read_to_string(&manifest).expect("a manifest");
    let text = text.replacen("metadata:\n", &format!("metadata:\n{metadata}"), 1);
    fs::write(&manifest, text).expect("rewrites the manifest");
}

#[test]
fn read_first_and_requires_must_name_files_of_the_skill_once_in_the_right_folders() {
    let host = host();
    let saved = host.service.create(&notes()).expect("creates");
    rewrite_metadata(
        &host,
        "release-notes",
        "  read-first: [references/style.md, references/none.md, scripts/x.py, references/style.md]\n  \
         requires: { references/style.md: [references/style.md], scripts/none.py: [assets/none.png, assets/logo.bin, assets/logo.bin] }\n",
    );
    let checked = host.service.get(&saved.skill.id).expect("re-reads");
    assert_eq!(checked.skill.status, SkillStatus::Invalid);
    assert_eq!(
        checked.skill.issues,
        [
            issue("readFirst.1", SkillIssueRule::Missing),
            issue("readFirst.2", SkillIssueRule::Path),
            issue("readFirst.3", SkillIssueRule::Duplicate),
            issue("requires.0", SkillIssueRule::Path),
            issue("requires.1", SkillIssueRule::Missing),
            issue("requires.1.0", SkillIssueRule::Missing),
            issue("requires.1.2", SkillIssueRule::Duplicate),
        ]
    );
    let other = host.service.create(&weather()).expect("creates weather");
    rewrite_metadata(&host, "weather", "  read-first: [references/a.md\n");
    assert_eq!(
        host.service
            .get(&other.skill.id)
            .expect("re-reads")
            .skill
            .issues,
        [issue("readFirst", SkillIssueRule::Manifest)]
    );
}

async fn loaded_output(host: &Host, key: &str) -> Value {
    let handle = host
        .runs
        .start(StartRunInput {
            kind: RunKind::Skill,
            subject_id: None,
            title: None,
            retry_of_id: None,
            graph: RunGraph {
                nodes: vec![GraphNode {
                    dependencies: Some(Vec::new()),
                    input: Some(json!({ "key": key })),
                    bindings: Some(std::collections::BTreeMap::new()),
                    retry: Some(RetryPolicy::default()),
                    ..GraphNode::new("probe", SKILL_LOAD)
                }],
                permission_scopes: Some(vec![format!("skill:{key}")]),
                allowed_tools: None,
            },
            input: None,
            concurrency: Some(1),
        })
        .expect("starts");
    probe_output(host, handle).await
}

#[tokio::test]
async fn skill_load_inlines_read_first_files_in_order_within_the_budget_and_describes_every_file() {
    let host = host();
    let text = |content: String, path: &str| SkillFileInput {
        path: path.to_owned(),
        content,
        encoding: None,
    };
    let long = |heading: &str, size: usize| format!("# {heading}\n\n{}", "x".repeat(size));
    host.service
        .create(&CreateSkillInput {
            files: Some(vec![
                text(long("Alpha", 10_000), "references/a.md"),
                text(long("Beta", 7_000), "references/b.md"),
                text("# Gamma\n\nshort".to_owned(), "references/c.md"),
                text(
                    "\"\"\"Build the page from a spec.\"\"\"\nimport sys\n".to_owned(),
                    "scripts/run.py",
                ),
                SkillFileInput {
                    path: "assets/logo.bin".to_owned(),
                    content: "AAEC".to_owned(),
                    encoding: Some(SkillFileEncoding::Base64),
                },
            ]),
            instructions: "Read with fs.read, run with shell.run.".to_owned(),
            ..notes()
        })
        .expect("creates");
    rewrite_metadata(
        &host,
        "release-notes",
        "  read-first: [assets/logo.bin, references/a.md, references/b.md, references/c.md]\n  \
         requires: { scripts/run.py: [references/b.md] }\n",
    );
    let output = loaded_output(&host, "release-notes").await;
    assert_eq!(output["inlined"], json!(["references/a.md"]));
    assert_eq!(
        output["required"],
        json!(["references/b.md", "references/c.md"])
    );
    assert_eq!(
        output["requires"],
        json!({ "scripts/run.py": ["references/b.md"] })
    );
    let purposes: Vec<(String, Value)> = output["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap_or_default().to_owned(),
                file["purpose"].clone(),
            )
        })
        .collect();
    assert_eq!(
        purposes,
        [
            ("assets/logo.bin".to_owned(), Value::Null),
            ("references/a.md".to_owned(), json!("Alpha")),
            ("references/b.md".to_owned(), json!("Beta")),
            ("references/c.md".to_owned(), json!("Gamma")),
            (
                "scripts/run.py".to_owned(),
                json!("Build the page from a spec.")
            ),
        ]
    );
    let text = output["text"].as_str().expect("a text");
    assert!(
        text.contains("<instructions>\nRead with fs.read, run with shell.run.\n</instructions>")
    );
    assert!(text.contains("<file path=\"skill:release-notes/references/a.md\">\n# Alpha"));
    assert!(!text.contains("# Beta"));
    assert!(!text.contains("<file path=\"skill:release-notes/assets/logo.bin\">"));
    assert!(text.contains(
        "read skill:release-notes/references/b.md, skill:release-notes/references/c.md with fs.read"
    ));
    assert!(text.contains("- references/a.md (10009 B): Alpha [included below]"));
    assert!(text.chars().count() < 12_000);
}

#[tokio::test]
async fn the_bundled_dashboard_arrives_with_its_spec_and_exploration_guide_in_one_call() {
    let host = host();
    host.service.library().install_bundled().expect("installs");
    let output = loaded_output(&host, "dashboard").await;
    assert_eq!(
        output["inlined"],
        json!(["references/spec.md", "references/exploration.md"])
    );
    assert_eq!(output["required"], json!([]));
    assert_eq!(
        output["requires"],
        json!({ "scripts/build_dashboard.py": ["references/spec.md"] })
    );
    assert!(
        output["files"]
            .as_array()
            .expect("files")
            .iter()
            .filter(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(".md")))
            .all(|file| file["purpose"].is_string())
    );
    let text = output["text"].as_str().expect("a text");
    assert!(text.chars().count() <= turn::SKILL_LOAD_RESULT_MAX_CHARS);
}

#[tokio::test]
async fn skill_load_and_skill_read_serve_the_instructions_and_stay_inside_the_folder() {
    let host = host();
    host.service.create(&notes()).expect("creates");
    let library = host.service.library();
    let loaded = library.load("release-notes").expect("loads");
    assert_eq!(
        loaded.instructions,
        "# Заметки\n\nСгруппируйте изменения по разделам."
    );
    assert_eq!(loaded.files.len(), 2);
    let text = library
        .read("release-notes", "references/style.md")
        .expect("reads a reference");
    assert_eq!(text.text.as_deref(), Some("Пишите кратко."));
    let binary = library
        .read("release-notes", "assets/logo.bin")
        .expect("reads an asset");
    assert_eq!((binary.size, binary.text), (3, None));
    for path in ["SKILL.md", "references/../SKILL.md", "references/none.md"] {
        let error = library.read("release-notes", path).expect_err(path);
        assert_eq!(error.message, SKILL_FILE_NOT_FOUND, "{path}");
    }
    let node = host.nodes.for_tool(SKILL_LOAD).expect("the load node");
    assert!(node.side_effect_free());
    let tools = host
        .tools
        .list(&zvs_contract::ToolListFilter::default())
        .expect("lists");
    let load = tools
        .iter()
        .find(|tool| tool.key == SKILL_LOAD)
        .expect("skill.load is a tool");
    assert!(load.read_only);
    assert_eq!(load.tier, ToolTier::Auto);
    assert!(tools.iter().all(|tool| tool.key != "skill.read"));
    assert!(tools.iter().all(|tool| tool.key != SKILL_GENERATE));
}

#[tokio::test]
async fn the_chat_catalogue_lists_usable_skills_with_the_wire_tool_names() {
    let host = host();
    host.service.create(&notes()).expect("creates notes");
    let skipped = host.service.create(&weather()).expect("creates weather");
    host.service
        .update(&UpdateSkillInput {
            enabled: Some(false),
            ..update(skipped.skill.id)
        })
        .expect("disables weather");
    let entries = host
        .service
        .catalogue("заметки", &CancellationToken::new())
        .await;
    assert_eq!(
        entries,
        [SkillCatalogueEntry {
            key: "release-notes".to_owned(),
            description: "Составляет заметки о выпуске по списку изменений".to_owned(),
        }]
    );
    let offered = crate::services::chat_tools::offered_tools(
        &[SKILL_LOAD.to_owned(), crate::files::FS_READ.to_owned()],
        &host
            .tools
            .list(&zvs_contract::ToolListFilter::default())
            .expect("lists"),
    );
    let text = catalogue_text(&entries, &offered);
    assert!(text.contains("call skill__load with its key"));
    assert!(text.contains("at skill:<key>/<path> only"));
    assert!(text.ends_with(
        "<skills>\n- release-notes: Составляет заметки о выпуске по списку изменений\n</skills>"
    ));
}

#[test]
fn the_bundled_skills_install_once_as_ready_skills_with_their_scripts() {
    let host = host();
    let library = host.service.library();
    let installed = library.install_bundled().expect("installs");
    assert_eq!(
        installed,
        BUNDLED_SKILLS
            .iter()
            .map(|skill| skill.key)
            .collect::<Vec<_>>()
    );
    let skills = host.service.list().expect("lists");
    for bundled in BUNDLED_SKILLS {
        let skill = skills
            .iter()
            .find(|skill| skill.key == bundled.key)
            .expect("a bundled skill is listed");
        assert_eq!(skill.source, SkillSource::Bundled);
        assert_eq!(skill.status, SkillStatus::Ready, "{}", skill.key);
        assert!(skill.enabled);
        assert!(!skill.description.is_empty());
        assert_eq!(skill.file_count, bundled.files.len() as u64);
        let loaded = library.load(bundled.key).expect("loads");
        assert!(service::has_scripts(&loaded), "{}", skill.key);
        assert!(!loaded.instructions.starts_with("---"));
        for file in bundled.files {
            assert!(
                host.workspace
                    .join(SKILLS_DIR)
                    .join(bundled.key)
                    .join(file.path)
                    .is_file(),
                "{}",
                file.path
            );
        }
    }
    assert!(
        library
            .install_bundled()
            .expect("installs again")
            .is_empty()
    );
    let pdf = skills
        .iter()
        .find(|skill| skill.key == "pdf")
        .expect("the pdf skill");
    host.service.remove(&pdf.id).expect("removes");
    assert!(
        library
            .install_bundled()
            .expect("installs after a removal")
            .is_empty()
    );
    assert!(
        host.service
            .list()
            .expect("lists")
            .iter()
            .all(|skill| skill.key != "pdf")
    );
}

#[test]
fn a_bundled_key_already_taken_by_the_user_is_left_alone() {
    let host = host();
    let mine = CreateSkillInput {
        key: "pdf".to_owned(),
        ..weather()
    };
    host.service.create(&mine).expect("creates");
    let installed = host.service.library().install_bundled().expect("installs");
    assert_eq!(
        installed,
        BUNDLED_SKILLS
            .iter()
            .map(|skill| skill.key)
            .filter(|key| *key != "pdf")
            .collect::<Vec<_>>()
    );
    let pdf = host
        .service
        .library()
        .find_by_key("pdf")
        .expect("finds")
        .expect("the user's skill");
    assert_eq!(pdf.source, SkillSource::Local);
    assert_eq!(pdf.name, "Погода");
}

#[test]
fn a_duplicate_copies_the_folder_under_a_free_key() {
    let host = host();
    let original = host.service.create(&notes()).expect("creates");
    let first = host
        .service
        .duplicate(&original.skill.id)
        .expect("duplicates");
    assert_eq!(first.skill.key, "release-notes-copy");
    assert_eq!(first.skill.name, "Заметки о выпуске (копия)");
    assert_eq!(first.skill.source, SkillSource::Local);
    assert_eq!(first.instructions, original.instructions);
    assert_eq!(first.files, original.files);
    assert_ne!(first.skill.id, original.skill.id);
    let second = host
        .service
        .duplicate(&original.skill.id)
        .expect("duplicates again");
    assert_eq!(second.skill.key, "release-notes-copy-2");
    let long = CreateSkillInput {
        key: "a".repeat(64),
        ..weather()
    };
    let saved = host.service.create(&long).expect("creates a long key");
    let copy = host.service.duplicate(&saved.skill.id).expect("duplicates");
    assert_eq!(copy.skill.key.len(), 64);
    assert!(copy.skill.key.ends_with("-copy"));
}

#[test]
fn enabling_does_not_rewrite_the_folder_and_works_on_an_invalid_skill() {
    let host = host();
    let saved = host.service.create(&notes()).expect("creates");
    let folder = host.workspace.join(SKILLS_DIR).join("release-notes");
    fs::write(folder.join("stray.txt"), "x").expect("drops a stray file");
    let manifest = folder.join(SKILL_FILE);
    let before = fs::metadata(&manifest)
        .and_then(|metadata| metadata.modified())
        .expect("a manifest");
    let disabled = host
        .service
        .set_enabled(&saved.skill.id, false)
        .expect("disables");
    assert!(!disabled.enabled);
    assert_eq!(disabled.status, SkillStatus::Invalid);
    assert_eq!(
        fs::metadata(&manifest)
            .and_then(|metadata| metadata.modified())
            .expect("a manifest"),
        before
    );
    let enabled = host
        .service
        .set_enabled(&saved.skill.id, true)
        .expect("enables");
    assert!(enabled.enabled);
}

#[test]
fn a_file_is_readable_by_id_even_when_the_skill_is_off_and_never_outside_its_folder() {
    let host = host();
    let saved = host.service.create(&notes()).expect("creates");
    host.service
        .set_enabled(&saved.skill.id, false)
        .expect("disables");
    let text = host
        .service
        .read_file(&saved.skill.id, "references/style.md")
        .expect("reads");
    assert_eq!(text.text.as_deref(), Some("Пишите кратко."));
    let binary = host
        .service
        .read_file(&saved.skill.id, "assets/logo.bin")
        .expect("reads a binary file");
    assert_eq!(binary.text, None);
    assert_eq!(binary.size, 3);
    for path in ["SKILL.md", "references/../SKILL.md", "references/none.md"] {
        let error = host
            .service
            .read_file(&saved.skill.id, path)
            .expect_err("refused");
        assert_eq!(error.code, AppErrorCode::NotFound, "{path}");
    }
}

const NOTES_V0: &[BundledSkill] = &[BundledSkill {
    key: "notes",
    manifest: "---\nname: notes\ndescription: Составляет заметки о выпуске\n---\n\nСгруппируйте изменения.\n",
    files: &[BundledFile {
        path: "references/style.md",
        content: "Кратко.",
    }],
}];

const NOTES_V1: &[BundledSkill] = &[BundledSkill {
    key: "notes",
    manifest: "---\nname: notes\ndescription: Составляет заметки о выпуске\nmetadata:\n  version: \"1.0.0\"\n  author: ZVS AI Studio\n---\n\nСгруппируйте изменения.\n",
    files: &[BundledFile {
        path: "references/style.md",
        content: "Кратко.",
    }],
}];

const NOTES_V2: &[BundledSkill] = &[BundledSkill {
    key: "notes",
    manifest: "---\nname: notes\ndescription: Составляет заметки о выпуске\nmetadata:\n  version: \"1.1.0\"\n  author: ZVS AI Studio\n---\n\nСгруппируйте изменения по разделам.\n",
    files: &[BundledFile {
        path: "references/style.md",
        content: "Кратко и по делу.",
    }],
}];

fn library_of(host: &Host, bundled: &'static [BundledSkill]) -> SkillLibrary {
    SkillLibrary::new(SkillLibraryOptions {
        bundled,
        ..SkillLibraryOptions::new(host.database.shared(), host.workspace.join(SKILLS_DIR))
    })
}

fn notes_in(library: &SkillLibrary) -> SkillDto {
    library
        .list()
        .expect("lists")
        .into_iter()
        .find(|skill| skill.key == "notes")
        .expect("the notes skill")
}

#[test]
fn versions_compare_by_their_numeric_parts() {
    assert!(is_newer("1.0.0", None));
    assert!(is_newer("1.0.0", Some("бессмыслица")));
    assert!(is_newer("1.10.0", Some("1.9.3")));
    assert!(is_newer("v2", Some("1.9")));
    assert!(is_newer("1.0.1-beta", Some("1.0.0")));
    assert!(!is_newer("1.0.0", Some("1.0.0")));
    assert!(!is_newer("1.0.0", Some("1.2.0")));
    assert!(!is_newer("бессмыслица", None));
    assert!(!is_newer("1.0.0.0", None));
}

#[test]
fn every_bundled_skill_declares_its_version_and_author_and_ships_every_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    for skill in BUNDLED_SKILLS {
        let manifest = manifest::parse_manifest(skill.manifest).expect("parses");
        assert_eq!(manifest.name, skill.key);
        let version = skill.version().expect("a version");
        assert!(bundled::version_order(&version).is_some(), "{}", skill.key);
        assert!(manifest.meta("author").is_some(), "{}", skill.key);
        let base = root.join(skill.key);
        let mut on_disk = Vec::new();
        let mut pending = vec![base.clone()];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir).expect("reads the skill folder") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    if !path.ends_with("__pycache__") {
                        pending.push(path);
                    }
                    continue;
                }
                let relative = path
                    .strip_prefix(&base)
                    .expect("inside the skill")
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                if relative != SKILL_FILE {
                    on_disk.push(relative);
                }
            }
        }
        on_disk.sort();
        let mut listed: Vec<String> = skill
            .files
            .iter()
            .map(|file| file.path.to_owned())
            .collect();
        listed.sort();
        assert_eq!(listed, on_disk, "{}", skill.key);
    }
}

#[test]
fn a_newer_bundled_version_is_offered_and_replaces_the_skill_in_place() {
    let host = host();
    let first = library_of(&host, NOTES_V1);
    assert_eq!(first.install_bundled().expect("installs"), ["notes"]);
    let installed = notes_in(&first);
    assert_eq!(installed.version.as_deref(), Some("1.0.0"));
    assert_eq!(installed.author.as_deref(), Some("ZVS AI Studio"));
    assert_eq!(installed.update, None);
    first
        .set_enabled(&installed.id, false)
        .expect("turns the skill off");

    let next = library_of(&host, NOTES_V2);
    assert!(next.install_bundled().expect("installs").is_empty());
    assert_eq!(
        notes_in(&next).update,
        Some(SkillUpdateDto {
            version: "1.1.0".to_owned(),
            modified: false,
        })
    );
    let updated = next.update_bundled(&installed.id).expect("updates");
    assert_eq!(updated.skill.id, installed.id);
    assert_eq!(updated.skill.version.as_deref(), Some("1.1.0"));
    assert_eq!(updated.skill.update, None);
    assert!(!updated.skill.enabled);
    assert_eq!(updated.skill.source, SkillSource::Bundled);
    assert_eq!(updated.instructions, "Сгруппируйте изменения по разделам.");
    let style = next
        .read_file(&installed.id, "references/style.md")
        .expect("reads");
    assert_eq!(style.text.as_deref(), Some("Кратко и по делу."));
    assert_eq!(notes_in(&next).update, None);

    let error = next
        .update_bundled(&installed.id)
        .expect_err("nothing to update");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, SKILL_UP_TO_DATE);
}

#[test]
fn a_bundled_skill_changed_by_the_user_reports_the_update_as_modified() {
    let host = host();
    library_of(&host, NOTES_V1)
        .install_bundled()
        .expect("installs");
    fs::write(
        host.workspace
            .join(SKILLS_DIR)
            .join("notes")
            .join("references")
            .join("style.md"),
        "Моя версия.",
    )
    .expect("edits a file");
    let next = library_of(&host, NOTES_V2);
    assert_eq!(
        notes_in(&next).update,
        Some(SkillUpdateDto {
            version: "1.1.0".to_owned(),
            modified: true,
        })
    );
    let listed = next.update_all_bundled().expect("updates everything");
    let skill = listed
        .iter()
        .find(|skill| skill.key == "notes")
        .expect("the notes skill");
    assert_eq!(skill.version.as_deref(), Some("1.1.0"));
    assert_eq!(skill.update, None);
}

#[test]
fn a_skill_seeded_before_versions_existed_is_offered_the_update() {
    let host = host();
    library_of(&host, NOTES_V0)
        .install_bundled()
        .expect("installs");
    fs::write(
        host.workspace.join(SKILLS_DIR).join(".bundled.json"),
        "[\"notes\"]",
    )
    .expect("writes the old marker");
    let next = library_of(&host, NOTES_V1);
    assert!(next.install_bundled().expect("installs").is_empty());
    let skill = notes_in(&next);
    assert_eq!(skill.version, None);
    assert_eq!(
        skill.update,
        Some(SkillUpdateDto {
            version: "1.0.0".to_owned(),
            modified: true,
        })
    );
    let updated = next.update_bundled(&skill.id).expect("updates");
    assert_eq!(updated.skill.version.as_deref(), Some("1.0.0"));
}

#[test]
fn a_user_skill_sharing_a_bundled_key_is_never_offered_an_update() {
    let host = host();
    host.service
        .create(&CreateSkillInput {
            key: "notes".to_owned(),
            ..weather()
        })
        .expect("creates");
    let next = library_of(&host, NOTES_V2);
    assert!(next.install_bundled().expect("installs").is_empty());
    let skill = notes_in(&next);
    assert_eq!(skill.source, SkillSource::Local);
    assert_eq!(skill.update, None);
    assert_eq!(
        next.update_bundled(&skill.id)
            .expect_err("not bundled")
            .code,
        AppErrorCode::Conflict
    );
}

fn authored(key: &str) -> AuthoredSkill {
    AuthoredSkill {
        key: key.to_owned(),
        name: "Отчёт о провайдерах".to_owned(),
        description: "Составляет отчёт о состоянии провайдеров".to_owned(),
        instructions: "# Отчёт\n\nПеречислите провайдеров.".to_owned(),
        files: vec![(
            "references/format.md".to_owned(),
            "Таблица: провайдер, состояние.".to_owned(),
        )],
        ..AuthoredSkill::default()
    }
}

#[test]
fn skill_save_is_a_destructive_tool_that_asks_before_every_call() {
    let host = host();
    let tool = host
        .tools
        .list(&zvs_contract::ToolListFilter::default())
        .expect("lists")
        .into_iter()
        .find(|tool| tool.key == SKILL_SAVE)
        .expect("skill.save is registered");
    assert!(tool.destructive);
    assert!(!tool.read_only);
    assert_eq!(tool.tier, ToolTier::Ask);
    assert!(!SKILL_TOOLS.contains(&SKILL_SAVE));
}

#[test]
fn an_authored_skill_is_saved_as_a_local_versioned_skill_with_its_files() {
    let host = host();
    let library = host.service.library();
    let saved = library.author(&authored("provider-report")).expect("saves");
    assert_eq!(saved.skill.source, SkillSource::Local);
    assert_eq!(saved.skill.status, SkillStatus::Ready);
    assert_eq!(saved.skill.version.as_deref(), Some("1.0.0"));
    assert!(saved.skill.enabled);
    assert_eq!(saved.skill.name, "Отчёт о провайдерах");
    assert_eq!(saved.instructions, "# Отчёт\n\nПеречислите провайдеров.");
    assert_eq!(
        saved
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["references/format.md"]
    );
    let manifest = fs::read_to_string(
        host.workspace
            .join(SKILLS_DIR)
            .join("provider-report")
            .join(SKILL_FILE),
    )
    .expect("reads the manifest");
    assert!(manifest.contains("version: \"1.0.0\""));

    let error = library
        .author(&authored("provider-report"))
        .expect_err("the key is taken");
    assert_eq!(issues_of(&error), [issue("key", SkillIssueRule::Duplicate)]);

    let replaced = library
        .author(&AuthoredSkill {
            replace: true,
            version: Some("1.1.0".to_owned()),
            files: Vec::new(),
            ..authored("provider-report")
        })
        .expect("replaces");
    assert_eq!(replaced.skill.id, saved.skill.id);
    assert_eq!(replaced.skill.version.as_deref(), Some("1.1.0"));
    assert!(replaced.files.is_empty());
}

#[test]
fn an_authored_skill_keeps_its_read_first_files_and_script_requirements() {
    let host = host();
    let library = host.service.library();
    let requires = vec![(
        "scripts/build.py".to_owned(),
        vec!["references/format.md".to_owned()],
    )];
    let saved = library
        .author(&AuthoredSkill {
            files: vec![
                (
                    "references/format.md".to_owned(),
                    "# Format\n\nТаблица.".to_owned(),
                ),
                (
                    "scripts/build.py".to_owned(),
                    "\"\"\"Build.\"\"\"\n".to_owned(),
                ),
            ],
            read_first: vec!["references/format.md".to_owned()],
            requires: requires.clone(),
            ..authored("provider-report")
        })
        .expect("saves");
    assert_eq!(saved.skill.status, SkillStatus::Ready);
    let loaded = library.load("provider-report").expect("loads");
    assert_eq!(loaded.read_first, ["references/format.md"]);
    assert_eq!(loaded.requires, requires);
    let error = library
        .author(&AuthoredSkill {
            read_first: vec!["references/none.md".to_owned()],
            ..authored("other-report")
        })
        .expect_err("a missing read-first file is refused");
    assert_eq!(
        issues_of(&error),
        [issue("readFirst.0", SkillIssueRule::Missing)]
    );
}

#[test]
fn an_authored_skill_never_overwrites_a_bundled_one_or_writes_outside_its_folders() {
    let host = host();
    let library = host.service.library();
    library.install_bundled().expect("installs");
    let error = library
        .author(&AuthoredSkill {
            replace: true,
            ..authored("pdf")
        })
        .expect_err("bundled skills are protected");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, SKILL_BUNDLED_PROTECTED);

    let error = library
        .author(&AuthoredSkill {
            files: vec![("notes.md".to_owned(), "x".to_owned())],
            ..authored("provider-report")
        })
        .expect_err("a file outside the skill folders is refused");
    assert_eq!(issues_of(&error), [issue("files.0", SkillIssueRule::Path)]);
    let explained = nodes::explained(error);
    assert!(explained.message.ends_with(": files.0 (path)"));
}
