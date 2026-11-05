use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{
    AppErrorCode, AuthMode, ChatToolCallStatus, HostEventDraft, ModelCapability,
    ProviderCapability, ProviderKind, ProviderStatus, RunDto, RunHandleDto, RunId, RunKind,
    RunListFilter, RunStatus, SUBAGENT_DOMAIN, SubagentModelDto, SubagentPreferencesDto,
    ToolSource, ToolTier,
};

use super::{Loop, TextOutput, call, call_with_id, text, within};
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft, ProviderPatch};
use crate::drivers::ai::{ChatRole, GenerateRequest, TextDelta};
use crate::kernel::{FnNode, PermissionRequirement, StepContext};
use crate::platform::clock::SystemClock;
use crate::services::chat::subagent::{
    AGENT_DELEGATE, DELEGATE_NAME, REPORT_TAG, SUBAGENT_HEADING, SUBAGENT_STOPPED,
};
use crate::services::chat::tests::eventually;
use crate::services::chat_ask::ASK_TOOL_NAME;
use crate::services::subagents::{BUDGET_CALLS_KEY, SubagentService};
use crate::tools::ToolDescriptor;

const REPORT: &str = "Нашёл: x объявлен в lib.rs";

#[derive(Default)]
struct Probe {
    inflight: AtomicUsize,
    peak: AtomicUsize,
    reads: AtomicUsize,
    hold: AtomicBool,
}

fn is_child(request: &GenerateRequest) -> bool {
    request
        .system
        .as_deref()
        .is_some_and(|system| system.starts_with(SUBAGENT_HEADING))
}

fn has_results(request: &GenerateRequest) -> bool {
    request
        .messages
        .iter()
        .any(|message| message.role == ChatRole::Tool)
}

fn delegate(task: &str, preset: &str, id: &str) -> TextDelta {
    call_with_id(
        DELEGATE_NAME,
        &json!({ "task": task, "preset": preset }),
        id,
    )
}

fn read() -> TextDelta {
    call("fs__read", &json!({ "path": "lib.rs" }))
}

impl Loop {
    fn with_subagents() -> (Self, Arc<Probe>) {
        let test = Self::new();
        let probe = Arc::new(Probe::default());
        let reads = Arc::clone(&probe);
        test.harness
            .runs
            .registry()
            .register(FnNode::new(
                "fs.read",
                PermissionRequirement::tool("fs.read"),
                false,
                move |context: StepContext, _input: Value| {
                    let probe = Arc::clone(&reads);
                    async move {
                        probe.reads.fetch_add(1, Ordering::SeqCst);
                        let now = probe.inflight.fetch_add(1, Ordering::SeqCst) + 1;
                        probe.peak.fetch_max(now, Ordering::SeqCst);
                        if probe.hold.load(Ordering::SeqCst) {
                            context.cancel.cancelled().await;
                        } else {
                            tokio::time::sleep(Duration::from_millis(120)).await;
                        }
                        probe.inflight.fetch_sub(1, Ordering::SeqCst);
                        Ok(TextOutput {
                            text: "pub fn x()".to_owned(),
                        })
                    }
                },
            ))
            .expect("registers a reader");
        test.tools
            .register(&ToolDescriptor {
                description: "Read a file".to_owned(),
                input_schema: json!({ "type": "object", "properties": { "path": { "type": "string" } } }),
                read_only: true,
                tier: Some(ToolTier::Auto),
                ..ToolDescriptor::new("fs.read", ToolSource::Builtin)
            })
            .expect("registers fs.read");
        test.preferences()
            .set(&SubagentPreferencesDto::default())
            .expect("enables subagents");
        (test, probe)
    }

    fn preferences(&self) -> SubagentService {
        SubagentService::new(Arc::clone(&self.harness.database), Arc::new(SystemClock))
    }

    fn dispatch(
        &self,
        rounds: usize,
        parent: impl Fn(&GenerateRequest) -> Vec<TextDelta> + Send + Sync + 'static,
        child: impl Fn(&GenerateRequest) -> Vec<TextDelta> + Send + Sync + 'static,
    ) {
        let parent = Arc::new(parent);
        let child = Arc::new(child);
        for _ in 0..rounds {
            let parent = Arc::clone(&parent);
            let child = Arc::clone(&child);
            self.driver.round(move |request| {
                if is_child(request) {
                    child(request)
                } else {
                    parent(request)
                }
            });
        }
    }

    fn children(&self, parent: &RunHandleDto) -> Vec<RunDto> {
        self.harness
            .runs
            .list(&RunListFilter {
                parent_run_id: Some(parent.id),
                ..RunListFilter::default()
            })
            .expect("lists")
            .items
            .into_iter()
            .map(|summary| self.harness.runs.get(&summary.id).expect("a run"))
            .collect()
    }

    fn seed_provider(&self, account: bool) -> String {
        self.harness
            .database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let base = ProviderDraft::new(
                    ProviderKind::Ollama,
                    "Cheap",
                    "http://localhost:11500",
                    vec![ProviderCapability::Text],
                    1,
                );
                let draft = if account {
                    ProviderDraft {
                        auth_mode: AuthMode::Account,
                        status: ProviderStatus::NeedsRelink,
                        ..base
                    }
                } else {
                    base
                };
                let provider = repositories.providers.create(&draft)?;
                repositories.models.replace_for_provider(
                    &provider.id,
                    &[ModelDraft {
                        context_window: Some(16384),
                        max_output: Some(512),
                        capabilities: vec![ModelCapability::Tools],
                        ..ModelDraft::new("cheap-model", "Cheap", 1)
                    }],
                    1,
                )?;
                Ok(provider.id)
            })
            .expect("seeds a provider")
    }

    fn prefer(&self, provider: &str, concurrency: u64) {
        self.preferences()
            .set(&SubagentPreferencesDto {
                enabled: true,
                models: vec![SubagentModelDto {
                    provider_id: provider.to_owned(),
                    model_id: "cheap-model".to_owned(),
                }],
                concurrency,
            })
            .expect("stores the preferences");
    }

    fn subagent_steps(&self, handle: &RunHandleDto) -> Vec<serde_json::Map<String, Value>> {
        self.harness
            .events()
            .into_iter()
            .filter(|event| event.stream_id == handle.stream_id)
            .filter_map(|event| match event.body {
                HostEventDraft::Step { step }
                    if step.get("domain").and_then(Value::as_str) == Some(SUBAGENT_DOMAIN) =>
                {
                    Some(step)
                }
                _ => None,
            })
            .collect()
    }
}

fn reporting_child(request: &GenerateRequest) -> Vec<TextDelta> {
    if has_results(request) {
        vec![text(REPORT)]
    } else {
        vec![read()]
    }
}

#[tokio::test]
async fn an_explore_delegate_runs_a_child_run_and_returns_a_fenced_report() {
    let (test, probe) = Loop::with_subagents();
    test.dispatch(
        4,
        |request| {
            if has_results(request) {
                vec![text("Ответ по отчёту.")]
            } else {
                vec![delegate("Найди x\nподробно", "explore", "call_d1")]
            }
        },
        reporting_child,
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read", "demo.lookup"]);

    let handle = test.turn(created, "Где объявлен x?").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(probe.reads.load(Ordering::SeqCst), 1);
    let children = test.children(&handle);
    assert_eq!(children.len(), 1);
    let child = &children[0];
    assert_eq!(child.kind, RunKind::Subagent);
    assert_eq!(child.parent_run_id, Some(handle.id));
    assert_eq!(
        child.subject_id.as_deref(),
        Some(created.to_string().as_str())
    );
    assert_eq!(child.status, RunStatus::Succeeded);
    assert_eq!(child.title.as_deref(), Some("Найди x"));

    let requests = test.driver.requests();
    let parent_tools: Vec<&str> = requests[0]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert!(parent_tools.contains(&DELEGATE_NAME));
    let child_request = requests
        .iter()
        .find(|request| is_child(request))
        .expect("a child request");
    let child_tools: Vec<&str> = child_request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(child_tools, ["fs__read"]);
    assert!(!child_tools.contains(&ASK_TOOL_NAME));
    assert!(!child_tools.contains(&DELEGATE_NAME));
    assert!(!child_tools.contains(&"demo__lookup"));

    let answered = requests.last().expect("the final parent request");
    let report = answered
        .messages
        .iter()
        .find(|message| message.role == ChatRole::Tool)
        .expect("the delegate result");
    assert!(report.content.contains(&format!(
        "<{REPORT_TAG} preset=\"explore\" status=\"ok\" partial=\"false\">"
    )));
    assert!(report.content.contains(REPORT));

    let record = test
        .harness
        .last(created)
        .tool_calls
        .into_iter()
        .find(|call| call.key == AGENT_DELEGATE)
        .expect("the delegate call");
    assert_eq!(record.status, ChatToolCallStatus::Succeeded);
    let meta = record.meta.expect("meta");
    assert_eq!(meta["runId"], json!(child.id.to_string()));
    assert_eq!(meta["preset"], json!("explore"));
    assert_eq!(meta["model"], json!("fake-model"));
    assert_eq!(meta["modelSource"], json!("dialog"));
    assert_eq!(meta["status"], json!("ok"));
    assert_eq!(meta["partial"], json!(false));
    assert_eq!(meta["toolCalls"], json!(1));
    assert!(meta["tokens"].as_u64().is_some_and(|tokens| tokens > 0));
    assert_eq!(meta["report"], json!(REPORT));

    let steps = test.subagent_steps(&handle);
    assert!(!steps.is_empty());
    assert!(
        steps
            .iter()
            .all(|step| step["runId"] == json!(child.id.to_string()))
    );
    assert!(
        steps
            .iter()
            .any(|step| step["call"]["key"] == json!("fs.read")
                && step["call"]["status"] == json!("succeeded"))
    );
    assert!(test.harness.events().iter().all(|event| {
        event.stream_id != handle.stream_id
            || !matches!(&event.body, HostEventDraft::Token { delta, .. } if delta.contains(REPORT))
    }));
}

#[tokio::test]
async fn explore_delegates_run_in_parallel_up_to_the_limit() {
    let (test, probe) = Loop::with_subagents();
    test.preferences()
        .set(&SubagentPreferencesDto {
            concurrency: 3,
            ..SubagentPreferencesDto::default()
        })
        .expect("stores");
    test.dispatch(
        10,
        |request| {
            if has_results(request) {
                vec![text("Готово.")]
            } else {
                vec![
                    delegate("Первое", "explore", "call_d1"),
                    delegate("Второе", "explore", "call_d2"),
                    delegate("Третье", "explore", "call_d3"),
                ]
            }
        },
        reporting_child,
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);

    let handle = test.turn(created, "Исследуй три места").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.children(&handle).len(), 3);
    assert!(probe.peak.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn an_account_model_runs_children_one_at_a_time() {
    let (test, probe) = Loop::with_subagents();
    let account = test.seed_provider(true);
    test.prefer(&account, 3);
    test.dispatch(
        10,
        |request| {
            if has_results(request) {
                vec![text("Готово.")]
            } else {
                vec![
                    delegate("Первое", "explore", "call_d1"),
                    delegate("Второе", "explore", "call_d2"),
                    delegate("Третье", "explore", "call_d3"),
                ]
            }
        },
        reporting_child,
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);

    let handle = test.turn(created, "Исследуй три места").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(probe.reads.load(Ordering::SeqCst), 3);
    assert_eq!(probe.peak.load(Ordering::SeqCst), 1);
    let record = test
        .harness
        .last(created)
        .tool_calls
        .into_iter()
        .find(|call| call.key == AGENT_DELEGATE)
        .expect("a delegate call");
    let meta = record.meta.expect("meta");
    assert_eq!(meta["modelSource"], json!("preferred"));
    assert_eq!(meta["model"], json!("cheap-model"));
}

#[tokio::test]
async fn a_preferred_model_falls_back_to_the_dialog_model_once_it_is_gone() {
    let (test, _probe) = Loop::with_subagents();
    let cheap = test.seed_provider(false);
    test.prefer(&cheap, 2);
    test.harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).providers.update(
                &cheap,
                &ProviderPatch {
                    enabled: Some(false),
                    ..ProviderPatch::default()
                },
                2,
            )?;
            Ok(())
        })
        .expect("disables");
    test.dispatch(
        4,
        |request| {
            if has_results(request) {
                vec![text("Готово.")]
            } else {
                vec![delegate("Найди x", "explore", "call_d1")]
            }
        },
        reporting_child,
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);

    test.turn(created, "Где x?").await;

    let meta = test
        .harness
        .last(created)
        .tool_calls
        .into_iter()
        .find(|call| call.key == AGENT_DELEGATE)
        .and_then(|call| call.meta)
        .expect("meta");
    assert_eq!(meta["modelSource"], json!("dialog"));
    assert_eq!(meta["model"], json!("fake-model"));
    assert!(test.preferences().get().expect("reads").models.is_empty());
}

#[tokio::test]
async fn a_child_out_of_budget_returns_a_partial_report() {
    let (test, _probe) = Loop::with_subagents();
    test.harness
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection)
                .settings
                .set(BUDGET_CALLS_KEY, "1", 1)?)
        })
        .expect("stores the budget");
    test.dispatch(
        6,
        |request| {
            if has_results(request) {
                vec![text("Частично.")]
            } else {
                vec![delegate("Найди всё", "explore", "call_d1")]
            }
        },
        |request| {
            if has_results(request) {
                vec![text("Успел прочитать один файл")]
            } else {
                vec![read()]
            }
        },
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);

    let handle = test.turn(created, "Найди всё").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let meta = test
        .harness
        .last(created)
        .tool_calls
        .into_iter()
        .find(|call| call.key == AGENT_DELEGATE)
        .and_then(|call| call.meta)
        .expect("meta");
    assert_eq!(meta["status"], json!("partial"));
    assert_eq!(meta["partial"], json!(true));
    let last = test
        .driver
        .requests()
        .into_iter()
        .rev()
        .find(|request| !is_child(request))
        .expect("a parent request");
    assert!(
        last.messages
            .iter()
            .any(|message| message.role == ChatRole::Tool
                && message.content.contains("partial=\"true\""))
    );
}

#[tokio::test]
async fn stopping_one_child_leaves_the_parent_running() {
    let (test, probe) = Loop::with_subagents();
    probe.hold.store(true, Ordering::SeqCst);
    test.dispatch(
        4,
        |request| {
            if has_results(request) {
                vec![text("Без отчёта.")]
            } else {
                vec![delegate("Долгая задача", "explore", "call_d1")]
            }
        },
        reporting_child,
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);
    let handle = test.harness.send(created, "Начни");
    eventually(|| probe.inflight.load(Ordering::SeqCst) == 1).await;
    let child = test.children(&handle).remove(0);

    test.harness.runs.cancel(&child.id).await.expect("cancels");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(
        test.harness.runs.get(&child.id).expect("a run").status,
        RunStatus::Cancelled
    );
    let record = test
        .harness
        .last(created)
        .tool_calls
        .into_iter()
        .find(|call| call.key == AGENT_DELEGATE)
        .expect("a delegate call");
    assert_eq!(record.status, ChatToolCallStatus::Cancelled);
    assert_eq!(record.error.as_deref(), Some(SUBAGENT_STOPPED));
    assert_eq!(record.meta.expect("meta")["status"], json!("cancelled"));
}

#[tokio::test]
async fn cancelling_the_parent_cancels_every_child() {
    let (test, probe) = Loop::with_subagents();
    probe.hold.store(true, Ordering::SeqCst);
    test.dispatch(
        4,
        |_| vec![delegate("Долгая задача", "explore", "call_d1")],
        reporting_child,
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);
    let handle = test.harness.send(created, "Начни");
    eventually(|| probe.inflight.load(Ordering::SeqCst) == 1).await;
    let child = test.children(&handle).remove(0);

    within(test.harness.runs.cancel(&handle.id))
        .await
        .expect("cancels");

    assert_eq!(test.harness.status(&handle), RunStatus::Cancelled);
    within(test.harness.runs.wait(&child.id)).await;
    assert_eq!(
        test.harness.runs.get(&child.id).expect("a run").status,
        RunStatus::Cancelled
    );
}

#[tokio::test]
async fn a_failed_child_is_reported_to_the_model() {
    let (test, _probe) = Loop::with_subagents();
    test.dispatch(
        4,
        |request| {
            if has_results(request) {
                vec![text("Субагент не справился.")]
            } else {
                vec![delegate("Сломайся", "explore", "call_d1")]
            }
        },
        |_| panic!("the child driver broke"),
    );
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);

    let handle = test.turn(created, "Попробуй").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let record = test
        .harness
        .last(created)
        .tool_calls
        .into_iter()
        .find(|call| call.key == AGENT_DELEGATE)
        .expect("a delegate call");
    assert_eq!(record.status, ChatToolCallStatus::Failed);
    assert_eq!(record.meta.expect("meta")["status"], json!("failed"));
    assert_eq!(test.children(&handle)[0].status, RunStatus::Failed);
}

#[tokio::test]
async fn a_child_approval_reaches_the_parent_dialog_with_the_subagent_label() {
    let (test, _probe) = Loop::with_subagents();
    test.set_tier(&test.drop, ToolTier::Auto);
    test.dispatch(
        4,
        |request| {
            if has_results(request) {
                vec![text("Удалено.")]
            } else {
                vec![delegate("Удали временные файлы", "general", "call_d1")]
            }
        },
        |request| {
            if has_results(request) {
                vec![text("Удалил")]
            } else {
                vec![call("demo__drop", &json!({}))]
            }
        },
    );
    let created = test.conversation(&[AGENT_DELEGATE, "demo.drop"]);
    let handle = test.harness.send(created, "Почисти");

    let mut forwarded = None;
    eventually(|| {
        forwarded = test
            .harness
            .events()
            .into_iter()
            .find_map(|event| match event.body {
                HostEventDraft::Approval { request }
                    if event.stream_id == handle.stream_id && request.contains_key("subagent") =>
                {
                    Some(request)
                }
                _ => None,
            });
        forwarded.is_some()
    })
    .await;
    let request = forwarded.expect("a forwarded approval");
    assert_eq!(request["conversationId"], json!(created.to_string()));
    assert_eq!(request["subagent"]["label"], json!("Удали временные файлы"));
    let child = RunId::parse(request["runId"].as_str().expect("a run id")).expect("a run id");
    assert_eq!(request["subagent"]["runId"], json!(child.to_string()));
    assert_ne!(child, handle.id);
    let approval = request["id"].as_str().expect("an approval id").to_owned();
    assert_eq!(
        test.harness
            .runs
            .approve(&handle.id, &approval, false)
            .expect_err("the parent does not own it")
            .code,
        AppErrorCode::Conflict
    );
    test.harness
        .runs
        .approve(&child, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.calls.drops(), 1);
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
}

#[tokio::test]
async fn disabled_subagents_remove_the_delegate_tool() {
    let (test, _probe) = Loop::with_subagents();
    test.preferences()
        .set(&SubagentPreferencesDto {
            enabled: false,
            ..SubagentPreferencesDto::default()
        })
        .expect("stores");
    let created = test.conversation(&[AGENT_DELEGATE, "fs.read"]);

    test.turn(created, "Привет").await;

    let tools: Vec<String> = test.driver.requests()[0]
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    assert!(!tools.iter().any(|name| name == DELEGATE_NAME));
    assert!(tools.iter().any(|name| name == "fs__read"));
}
