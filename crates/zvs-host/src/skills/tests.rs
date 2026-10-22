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
    AppError, AppErrorCode, ChatToolCallStatus, CreateSkillInput, GraphNode, ProviderCapability,
    ProviderId, ProviderKind, RetryPolicy, RunGraph, RunKind, RunStatus, SKILL_TRIGGER_KEY,
    SkillExportInput, SkillFileEncoding, SkillFileInput, SkillImportInput, SkillIssueDto,
    SkillIssueRule, SkillMatchMethod, SkillModelDto, SkillRunInput, SkillSource, SkillStatus,
    SkillTestInput, StartRunInput, ToolDto, ToolSource, ToolTier, UpdateSkillInput, Validate,
};

use super::library::{SKILL_DISABLED, SKILL_FILE_NOT_FOUND};
use super::manifest::SKILL_FILE;
use super::nodes::SkillGenerateOutput;
use super::*;
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::data::schema::ProviderRow;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::{EmbeddingDriver, SharedTextDriver};
use crate::drivers::ai::{
    AiDriver, DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult, TextDelta,
    TextGenerationDriver, ToolCall,
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

impl Scripted {
    fn round(&self, deltas: Vec<TextDelta>) {
        self.rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(deltas);
    }

    fn requests(&self) -> Vec<GenerateRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
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
    driver: Arc<Scripted>,
    calls: Arc<Calls>,
    picker: Arc<FakePicker>,
    lookup: ToolDto,
    off: ToolDto,
    provider_id: ProviderId,
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
    let lookup = register("demo.lookup");
    register("demo.drop");
    let off = register("demo.off");
    let disabled = register("demo.disabled");
    tools
        .set_tier(&off.id.to_string(), ToolTier::Off, "test")
        .expect("turns a tool off");
    tools
        .set_enabled(&disabled.id.to_string(), false)
        .expect("disables a tool");
    let library = SkillLibrary::new(SkillLibraryOptions::new(
        database.shared(),
        workspace.join(SKILLS_DIR),
        tools.clone(),
    ));
    register_skill_nodes(
        &nodes,
        &library,
        SkillScriptOptions::new(None, workspace.join("skill-scripts")),
    )
    .expect("registers the skill nodes");
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
    let provider_id = database
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
            Ok(provider.id)
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
        ..SkillServiceOptions::new(library, runs.clone(), database.shared())
    });
    Host {
        database,
        workspace,
        tools,
        runs,
        nodes,
        service,
        driver,
        calls,
        picker,
        lookup,
        off,
        provider_id: ProviderId::parse(&provider_id).expect("a provider id"),
    }
}

fn notes(allowed: &[&str]) -> CreateSkillInput {
    CreateSkillInput {
        key: "release-notes".to_owned(),
        name: "Заметки о выпуске".to_owned(),
        description: "Составляет заметки о выпуске по списку изменений".to_owned(),
        instructions: "# Заметки\n\nСгруппируйте изменения по разделам.".to_owned(),
        allowed_tools: Some(allowed.iter().map(|key| (*key).to_owned()).collect()),
        model_override: None,
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
        allowed_tools: None,
        model_override: None,
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
    let detail = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("creates the skill");
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
    assert!(manifest.contains("allowed-tools: \"demo.lookup\""));
    assert_eq!(
        fs::read(folder.join("assets").join("logo.bin")).expect("an asset"),
        [0, 1, 2]
    );
    let usage = host
        .tools
        .usage(&host.lookup.id.to_string())
        .expect("usage");
    assert_eq!(usage.references[0].name, "Заметки о выпуске");
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

#[test]
fn allowed_tools_can_never_exceed_the_ceiling_on_the_tools_page() {
    let host = host();
    let error = host
        .service
        .create(&notes(&[
            "demo.lookup",
            "demo.off",
            "demo.disabled",
            "demo.missing",
        ]))
        .expect_err("the widening attempt is rejected");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, library::SKILL_INVALID);
    assert_eq!(
        issues_of(&error),
        [
            issue("allowedTools.1", SkillIssueRule::ToolOff),
            issue("allowedTools.2", SkillIssueRule::ToolDisabled),
            issue("allowedTools.3", SkillIssueRule::UnknownTool),
        ]
    );
    assert!(host.service.list().expect("lists").is_empty());
    assert!(
        !host
            .workspace
            .join(SKILLS_DIR)
            .join("release-notes")
            .exists()
    );
    let saved = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("a narrowed skill is accepted");
    let widened = host
        .service
        .update(&UpdateSkillInput {
            id: saved.skill.id,
            allowed_tools: Some(vec!["demo.lookup".to_owned(), host.off.key.clone()]),
            ..update(saved.skill.id)
        })
        .expect_err("widening an existing skill is rejected");
    assert_eq!(
        issues_of(&widened),
        [issue("allowedTools.1", SkillIssueRule::ToolOff)]
    );
    assert_eq!(
        host.service
            .get(&saved.skill.id)
            .expect("gets")
            .skill
            .allowed_tools,
        ["demo.lookup"]
    );
}

fn update(id: zvs_contract::SkillId) -> UpdateSkillInput {
    UpdateSkillInput {
        id,
        key: None,
        name: None,
        description: None,
        instructions: None,
        allowed_tools: None,
        model_override: None,
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
            allowed_tools: None,
            model_override: None,
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
    let saved = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("creates");
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
fn a_hand_edit_is_re_read_and_a_broken_or_widened_skill_is_reported_invalid() {
    let host = host();
    let saved = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("creates");
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
    fs::write(
        &manifest,
        "---\nname: release-notes\ndescription: Расширено\nallowed-tools: demo.lookup demo.off\n---\n\nТекст.\n",
    )
    .expect("widens by hand");
    let widened = host.service.get(&saved.skill.id).expect("re-reads");
    assert_eq!(widened.skill.status, SkillStatus::Invalid);
    assert_eq!(
        widened.skill.issues,
        [issue("allowedTools.1", SkillIssueRule::ToolOff)]
    );
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
fn turning_a_tool_off_later_invalidates_the_skills_that_allow_it() {
    let host = host();
    let saved = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("creates");
    host.tools
        .set_tier(&host.lookup.id.to_string(), ToolTier::Off, "user")
        .expect("turns the tool off");
    let listed = host.service.list().expect("lists");
    assert_eq!(listed[0].status, SkillStatus::Invalid);
    assert_eq!(
        listed[0].issues,
        [issue("allowedTools.0", SkillIssueRule::ToolOff)]
    );
    host.tools
        .set_tier(&host.lookup.id.to_string(), ToolTier::Auto, "user")
        .expect("turns the tool back on");
    assert_eq!(
        host.service
            .get(&saved.skill.id)
            .expect("gets")
            .skill
            .status,
        SkillStatus::Ready
    );
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
    let original = host
        .service
        .create(&CreateSkillInput {
            model_override: Some(Some(SkillModelDto {
                provider_id: host.provider_id,
                model_id: "fake-model".to_owned(),
            })),
            ..notes(&["demo.lookup"])
        })
        .expect("creates");
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
    assert_eq!(imported.skill.allowed_tools, original.skill.allowed_tools);
    assert_eq!(imported.skill.model_override, original.skill.model_override);
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
async fn a_foreign_skill_imports_with_its_unknown_tools_flagged_and_loose_files_moved() {
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
    assert_eq!(imported.skill.status, SkillStatus::Invalid);
    assert_eq!(
        imported.skill.issues,
        [
            issue("allowedTools.0", SkillIssueRule::UnknownTool),
            issue("allowedTools.1", SkillIssueRule::UnknownTool),
        ]
    );
    assert_eq!(imported.files[0].path, "references/REFERENCE.md");
    let manifest = fs::read_to_string(
        host.workspace
            .join(SKILLS_DIR)
            .join("pdf-tools")
            .join(SKILL_FILE),
    )
    .expect("a manifest");
    assert!(manifest.contains("license: \"MIT\""));
    let fixed = host
        .service
        .update(&UpdateSkillInput {
            allowed_tools: Some(vec!["demo.lookup".to_owned()]),
            ..update(imported.skill.id)
        })
        .expect("the editor fixes it");
    assert_eq!(fixed.skill.status, SkillStatus::Ready);
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
    let notes_skill = host.service.create(&notes(&[])).expect("creates notes");
    host.service.create(&weather()).expect("creates weather");
    let cancel = CancellationToken::new();
    let result = host
        .service
        .test(
            &SkillTestInput {
                message: "Составь заметки к выпуску 2.4".to_owned(),
                id: None,
                description: None,
                threshold: None,
            },
            &cancel,
        )
        .await
        .expect("scores");
    assert_eq!(result.method, SkillMatchMethod::Lexical);
    assert_eq!(
        result.threshold,
        zvs_contract::skills::DEFAULT_SKILL_THRESHOLD
    );
    assert_eq!(result.matches[0].key, "release-notes");
    assert!(result.matches[0].triggered);
    assert!(result.matches[0].score > result.matches[1].score);
    assert!(!result.matches[1].triggered);
    let draft = host
        .service
        .test(
            &SkillTestInput {
                message: "Какая погода завтра".to_owned(),
                id: Some(notes_skill.skill.id),
                description: Some("Прогноз погоды на завтра".to_owned()),
                threshold: Some(0.9),
            },
            &cancel,
        )
        .await
        .expect("scores a draft description");
    let drafted = draft
        .matches
        .iter()
        .find(|entry| entry.id == Some(notes_skill.skill.id))
        .expect("the draft is scored");
    assert!(drafted.score >= 0.6, "{}", drafted.score);
    assert!(!drafted.triggered);
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
    let picked = host
        .service
        .select("Прогноз погоды", &cancel)
        .await
        .expect("selects");
    assert_eq!(
        picked
            .iter()
            .map(|skill| skill.key.as_str())
            .collect::<Vec<_>>(),
        ["weather"]
    );
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
                    "catalogueLimit": 20,
                    "embedding": { "providerId": embedding, "modelId": "embed" }
                })
                .to_string(),
                1,
            )?;
            Ok(())
        })
        .expect("configures embeddings");
    host.service.create(&notes(&[])).expect("creates notes");
    host.service.create(&weather()).expect("creates weather");
    let result = host
        .service
        .test(
            &SkillTestInput {
                message: "нужен текст к выпуску".to_owned(),
                id: None,
                description: None,
                threshold: None,
            },
            &CancellationToken::new(),
        )
        .await
        .expect("scores");
    assert_eq!(result.method, SkillMatchMethod::Embedding);
    assert_eq!(result.matches[0].key, "release-notes");
    assert!(
        (result.matches[0].score - 0.995).abs() < 0.01,
        "{}",
        result.matches[0].score
    );
    assert!(result.matches[0].triggered);
    assert!(!result.matches[1].triggered);
}

#[tokio::test]
async fn a_skill_run_executes_with_the_narrowed_set_and_is_refused_a_tool_outside_it() {
    let host = host();
    let saved = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("creates");
    host.driver.round(vec![
        TextDelta::ToolCall(ToolCall::new(
            "call_1",
            "demo__lookup",
            json!({ "query": "2.4" }).to_string(),
        )),
        TextDelta::ToolCall(ToolCall::new("call_2", "demo__drop", json!({}).to_string())),
    ]);
    host.driver.round(vec![TextDelta::text("Заметки готовы.")]);
    let handle = host
        .service
        .run(&SkillRunInput {
            id: saved.skill.id,
            message: "Составь заметки к выпуску 2.4".to_owned(),
            provider_id: Some(host.provider_id),
            model_id: Some("fake-model".to_owned()),
        })
        .expect("starts");
    let run = finished(&host, &handle).await;
    assert_eq!(run.status, RunStatus::Succeeded, "{:?}", run.error);
    assert_eq!(run.kind, RunKind::Skill);
    assert_eq!(run.subject_id, Some(saved.skill.id.to_string()));
    assert_eq!(
        run.graph.allowed_tools,
        Some(vec!["demo.lookup".to_owned(), SKILL_READ.to_owned()])
    );
    assert_eq!(
        run.graph.permission_scopes,
        Some(vec!["skill:release-notes".to_owned()])
    );
    let first = &host.driver.requests()[0];
    let mut offered: Vec<&str> = first.tools.iter().map(|tool| tool.name.as_str()).collect();
    offered.sort_unstable();
    assert_eq!(offered, ["demo__lookup", "skill__read"]);
    let system = first.system.clone().unwrap_or_default();
    assert!(system.contains("Сгруппируйте изменения по разделам."));
    assert!(system.contains("references/style.md"));
    let step = host
        .runs
        .steps(&handle.id)
        .expect("steps")
        .into_iter()
        .find(|step| step.node_id == "skill")
        .expect("the skill step");
    let output: SkillGenerateOutput =
        serde_json::from_value(step.output.expect("an output")).expect("the output shape");
    assert_eq!(output.text, "Заметки готовы.");
    assert_eq!(output.calls[0].status, ChatToolCallStatus::Succeeded);
    assert_eq!(output.calls[1].status, ChatToolCallStatus::Refused);
    assert_eq!(
        *host
            .calls
            .lookups
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
        ["2.4"]
    );
    assert_eq!(
        *host
            .calls
            .drops
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
        0
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

#[tokio::test]
async fn a_run_needs_a_model_and_a_usable_skill() {
    let host = host();
    let saved = host
        .service
        .create(&notes(&["demo.lookup"]))
        .expect("creates");
    let input = SkillRunInput {
        id: saved.skill.id,
        message: "Составь заметки".to_owned(),
        provider_id: None,
        model_id: None,
    };
    let error = host.service.run(&input).expect_err("no model");
    assert_eq!(error.message, service::NO_MODEL);
    host.service
        .update(&UpdateSkillInput {
            model_override: Some(Some(SkillModelDto {
                provider_id: host.provider_id,
                model_id: "fake-model".to_owned(),
            })),
            enabled: Some(false),
            ..update(saved.skill.id)
        })
        .expect("updates");
    let disabled = host.service.run(&input).expect_err("a disabled skill");
    assert_eq!(disabled.message, SKILL_DISABLED);
    host.service
        .update(&UpdateSkillInput {
            enabled: Some(true),
            ..update(saved.skill.id)
        })
        .expect("updates");
    let handle = host
        .service
        .run(&input)
        .expect("the override model is used");
    assert_eq!(finished(&host, &handle).await.status, RunStatus::Succeeded);
}

#[tokio::test]
async fn skill_load_and_skill_read_serve_the_instructions_and_stay_inside_the_folder() {
    let host = host();
    host.service
        .create(&notes(&["demo.lookup"]))
        .expect("creates");
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
    assert!(tools.iter().any(|tool| tool.key == SKILL_READ));
    assert!(tools.iter().all(|tool| tool.key != SKILL_GENERATE));
}

#[tokio::test]
async fn the_chat_catalogue_lists_usable_skills_with_the_wire_tool_names() {
    let host = host();
    host.service.create(&notes(&[])).expect("creates notes");
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
        &[SKILL_LOAD.to_owned(), SKILL_READ.to_owned()],
        &host
            .tools
            .list(&zvs_contract::ToolListFilter::default())
            .expect("lists"),
    );
    let text = catalogue_text(&entries, &offered);
    assert!(text.contains("call skill__load with its key"));
    assert!(text.contains("with skill__read only"));
    assert!(text.ends_with(
        "<skills>\n- release-notes: Составляет заметки о выпуске по списку изменений\n</skills>"
    ));
}
