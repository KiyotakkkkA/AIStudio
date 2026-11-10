use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, ExecJudgeDto, ExecMode, ExecPolicyDto,
    FileChangeKind, FileDiffDto, HostEventDraft, ProcessPlanDto, ProviderCapability, ProviderKind,
    RunId, RunKind, ToolAuthorisationDto, ToolAuthorisationKind, ToolTier,
};

use super::stages::{FLOOR_DISABLED, FLOOR_OFF, FLOOR_UNLISTED, TIER_AUTO, TIER_GRANTED};
use super::{
    AccessPipeline, AccessRequest, Actor, Decision, Effect, FloorStage, OUTSIDE_ALLOWED_TOOLS,
    Outcome, PERMISSION_DENIED, Pass, STAGE_FAILED, Stage, StageName, Step, TOOL_DISABLED,
    TierStage,
};
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::kernel::{
    AdmissionOrigin, ApprovalDecision, ExecReview, KernelServices, NodeRegistry, StepContext,
    StepLink,
};
use crate::platform::ids::new_id;
use crate::policy::{
    ExecPolicyOptions, ExecPolicyService, Family, RULE_DENIED, ReviewVerdict, ReviewedAction,
    VENDOR_PACK,
};
use crate::services::permissions::{GLOBAL_SCOPE, PermissionService};
use crate::testing::{FakeClock, TemporaryDatabase, temporary_database};

#[derive(Clone, Copy)]
enum Act {
    Pass,
    Allow,
    Ask,
    Deny,
    Fail(AppErrorCode),
}

struct Scripted {
    name: StageName,
    act: Act,
    closed: bool,
    visited: Arc<Mutex<Vec<StageName>>>,
}

#[async_trait]
impl Stage for Scripted {
    fn name(&self) -> StageName {
        self.name
    }

    fn fails_closed(&self) -> bool {
        self.closed
    }

    async fn check(
        &self,
        _request: &AccessRequest,
        _pass: &mut Pass<'_>,
    ) -> Result<Step, AppError> {
        self.visited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.name);
        match self.act {
            Act::Pass => Ok(Step::Continue),
            Act::Allow => Ok(Step::Decide(Decision::allow(self.name))),
            Act::Ask => Ok(Step::Decide(Decision::ask(self.name))),
            Act::Deny => Ok(Step::Decide(Decision::deny(self.name))),
            Act::Fail(code) => Err(AppError::new(code, "stage failed")),
        }
    }
}

type Expected = Result<(Outcome, StageName, Option<&'static str>, Vec<StageName>), AppErrorCode>;

struct Case {
    name: &'static str,
    stages: Vec<(StageName, Act, bool)>,
    expected: Expected,
}

fn plain_request() -> AccessRequest {
    AccessRequest::new(Actor::run("run"), "tool", Effect::Plain)
}

#[tokio::test]
async fn the_pipeline_runs_stages_in_order_and_the_first_decision_wins() {
    use Act::{Allow, Ask, Deny, Fail, Pass as Next};
    use StageName::{Floor, Judge, Mode, Person, Rules, Tier};
    let cases = vec![
        Case {
            name: "the first decision ends the pipeline",
            stages: vec![
                (Floor, Next, true),
                (Tier, Allow, true),
                (Mode, Deny, false),
            ],
            expected: Ok((Outcome::Allow, Tier, None, vec![Floor, Tier])),
        },
        Case {
            name: "a floor deny is final",
            stages: vec![(Floor, Deny, true), (Tier, Allow, true)],
            expected: Ok((Outcome::Deny, Floor, None, vec![Floor])),
        },
        Case {
            name: "an ask from a later stage is kept",
            stages: vec![
                (Mode, Next, false),
                (Rules, Ask, false),
                (Judge, Allow, false),
            ],
            expected: Ok((Outcome::Ask, Rules, None, vec![Mode, Rules])),
        },
        Case {
            name: "no decision goes to the person",
            stages: vec![
                (Mode, Next, false),
                (Rules, Next, false),
                (Judge, Next, false),
            ],
            expected: Ok((Outcome::Ask, Person, None, vec![Mode, Rules, Judge])),
        },
        Case {
            name: "an empty pipeline asks the person",
            stages: Vec::new(),
            expected: Ok((Outcome::Ask, Person, None, Vec::new())),
        },
        Case {
            name: "an error in a decision stage asks",
            stages: vec![
                (Mode, Next, false),
                (Rules, Fail(AppErrorCode::Unknown), false),
                (Judge, Allow, false),
            ],
            expected: Ok((Outcome::Ask, Rules, Some(STAGE_FAILED), vec![Mode, Rules])),
        },
        Case {
            name: "an error in the floor is an error",
            stages: vec![
                (Floor, Fail(AppErrorCode::Unknown), true),
                (Tier, Allow, true),
            ],
            expected: Err(AppErrorCode::Unknown),
        },
        Case {
            name: "a cancelled run is an error in any stage",
            stages: vec![
                (Mode, Next, false),
                (Judge, Fail(AppErrorCode::RunCancelled), false),
            ],
            expected: Err(AppErrorCode::RunCancelled),
        },
    ];
    for case in cases {
        let visited = Arc::new(Mutex::new(Vec::new()));
        let stages: Vec<Box<dyn Stage>> = case
            .stages
            .iter()
            .map(|(name, act, closed)| {
                Box::new(Scripted {
                    name: *name,
                    act: *act,
                    closed: *closed,
                    visited: Arc::clone(&visited),
                }) as Box<dyn Stage>
            })
            .collect();
        let pipeline = AccessPipeline::new(stages);
        assert_eq!(
            pipeline.stages(),
            case.stages.iter().map(|stage| stage.0).collect::<Vec<_>>(),
            "{}",
            case.name
        );
        let result = pipeline
            .decide(&plain_request(), &mut Pass::default())
            .await;
        let visited = visited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match (result, case.expected) {
            (Ok(decision), Ok((outcome, stage, source, trail))) => {
                assert_eq!(decision.outcome, outcome, "{}", case.name);
                assert_eq!(decision.stage, stage, "{}", case.name);
                assert_eq!(decision.source.as_deref(), source, "{}", case.name);
                assert_eq!(decision.trail, trail, "{}", case.name);
                assert_eq!(visited, trail, "{}", case.name);
            }
            (Err(error), Err(code)) => assert_eq!(error.code, code, "{}", case.name),
            (result, _) => panic!("{}: unexpected {result:?}", case.name),
        }
    }
}

fn change(kind: FileChangeKind, path: &str) -> FileDiffDto {
    FileDiffDto {
        path: path.to_owned(),
        from: None,
        kind,
        added: 1,
        removed: 1,
        diff: String::new(),
        truncated: false,
    }
}

fn delete(path: &str) -> ReviewedAction {
    ReviewedAction::files(vec![change(FileChangeKind::Delete, path)], |_| false)
}

fn shell(argv: &[&str]) -> ReviewedAction {
    ReviewedAction::Process {
        tool: "shell.run".to_owned(),
        process: ProcessPlanDto {
            program: argv[0].to_owned(),
            language: None,
            argv: argv.iter().map(|part| (*part).to_owned()).collect(),
            cwd: "C:/work".to_owned(),
            source: None,
            packages: Vec::new(),
            background: false,
            reason: None,
        },
    }
}

#[test]
fn every_reviewed_action_becomes_an_effect_of_the_same_family() {
    let actions = [
        ReviewedAction::Plain,
        ReviewedAction::Declared,
        ReviewedAction::Browser,
        delete("C:/work/a.txt"),
        ReviewedAction::files(
            vec![change(FileChangeKind::Create, "C:/work/new.txt")],
            |_| false,
        ),
        shell(&["git", "status"]),
    ];
    for action in actions {
        let effect = Effect::from(action.clone());
        assert_eq!(effect.family(), action.family(), "{action:?}");
        assert_eq!(effect.reviewed(), action);
    }
}

#[test]
fn the_new_effects_take_the_family_of_what_they_replace() {
    let cases = [
        (
            Effect::Desktop {
                apps: vec!["notepad.exe".to_owned()],
                sensitive: true,
            },
            Family::Browser,
        ),
        (
            Effect::Desktop {
                apps: Vec::new(),
                sensitive: false,
            },
            Family::Benign,
        ),
        (
            Effect::Remote {
                service: "mail".to_owned(),
                action: "send".to_owned(),
            },
            Family::Declared,
        ),
        (Effect::Opaque { destructive: true }, Family::Declared),
        (Effect::Opaque { destructive: false }, Family::Benign),
    ];
    for (effect, family) in cases {
        assert_eq!(effect.family(), family, "{effect:?}");
        assert_eq!(effect.reviewed().family(), family, "{effect:?}");
    }
}

#[test]
fn a_decision_records_and_refuses_as_the_gate_did() {
    let auto = Decision::allow(StageName::Tier).from_source(TIER_AUTO);
    assert_eq!(
        auto.authorisation(),
        Some(ToolAuthorisationDto::of(ToolAuthorisationKind::Auto))
    );
    let granted = Decision::allow(StageName::Tier).from_source(TIER_GRANTED);
    assert_eq!(
        granted.authorisation(),
        Some(ToolAuthorisationDto::of(ToolAuthorisationKind::Granted))
    );
    assert_eq!(Decision::allow(StageName::Floor).authorisation(), None);
    assert_eq!(Decision::ask(StageName::Tier).authorisation(), None);
    let mode = Decision::allow(StageName::Mode).from_source("safety-ask");
    assert_eq!(
        mode.authorisation(),
        Some(ToolAuthorisationDto::of(ToolAuthorisationKind::Policy).from_source("safety-ask"))
    );
    let rule = Decision::allow(StageName::Rules)
        .from_source("vendor.allow")
        .because(Some("text".to_owned()));
    assert_eq!(
        rule.verdict(),
        ReviewVerdict::Run(
            ToolAuthorisationDto::of(ToolAuthorisationKind::Judge)
                .from_source("vendor.allow")
                .because(Some("text".to_owned()))
        )
    );
    let person = Decision::allow(StageName::Person).because(Some("why".to_owned()));
    assert_eq!(
        person.authorisation(),
        Some(
            ToolAuthorisationDto::of(ToolAuthorisationKind::Approved)
                .because(Some("why".to_owned()))
        )
    );

    let unlisted = Decision::deny(StageName::Floor)
        .from_source(FLOOR_UNLISTED)
        .refusal("fs.read");
    assert_eq!(unlisted.code, AppErrorCode::PermissionDenied);
    assert_eq!(unlisted.message, OUTSIDE_ALLOWED_TOOLS);
    assert_eq!(
        unlisted
            .details
            .as_ref()
            .and_then(|details| details.get("key")),
        Some(&serde_json::json!("fs.read"))
    );
    let disabled = Decision::deny(StageName::Floor)
        .from_source(FLOOR_DISABLED)
        .refusal("fs.read");
    assert_eq!(disabled.message, TOOL_DISABLED);
    let off = Decision::deny(StageName::Floor)
        .from_source(FLOOR_OFF)
        .refusal("fs.read");
    assert_eq!(off.message, PERMISSION_DENIED);
    assert_eq!(
        Decision::deny(StageName::Tier).refusal("fs.read").message,
        PERMISSION_DENIED
    );
    let denied = Decision::deny(StageName::Rules)
        .from_source("vendor.deny")
        .refusal("fs.apply");
    assert_eq!(denied.message, RULE_DENIED);
    assert_eq!(
        Decision::deny(StageName::Rules)
            .from_source("vendor.deny")
            .verdict(),
        ReviewVerdict::Deny {
            rule: "vendor.deny".to_owned()
        }
    );
}

struct Silent;

impl StepLink for Silent {
    fn emit(&self, _event: HostEventDraft) -> Result<(), AppError> {
        Ok(())
    }

    fn request_approval(
        &self,
        _request: ApprovalRequestDto,
    ) -> BoxFuture<'static, Result<ApprovalDecision, AppError>> {
        Box::pin(async { Ok(ApprovalDecision::Denied) })
    }
}

const EMPTY_PACK: &str = r#"{ "id": "test.empty", "allows": false, "rules": [] }"#;
const JUDGE_MODEL: &str = "judge-model";

struct Host {
    _database: TemporaryDatabase,
    permissions: Arc<PermissionService>,
    policy: ExecPolicyService,
    admission: AccessPipeline,
    judge: ExecJudgeDto,
}

impl Host {
    fn new(vendor: &str) -> Self {
        let database = temporary_database();
        let provider = database
            .shared()
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories.providers.create(&ProviderDraft::new(
                    ProviderKind::Ollama,
                    "Judge",
                    "http://localhost:11434",
                    vec![ProviderCapability::Text],
                    1,
                ))?;
                repositories.models.replace_for_provider(
                    &provider.id,
                    &[ModelDraft::new(JUDGE_MODEL, "Judge", 1)],
                    1,
                )?;
                Ok(provider.id)
            })
            .expect("seeds a provider");
        let clock = Arc::new(FakeClock::default());
        let permissions = Arc::new(PermissionService::new(
            database.shared(),
            Arc::new(NodeRegistry::new()),
            clock.clone(),
        ));
        let policy = ExecPolicyService::new(ExecPolicyOptions {
            vendor: vendor.to_owned(),
            ..ExecPolicyOptions::new(database.shared(), clock)
        });
        let admission = AccessPipeline::new(vec![
            Box::new(FloorStage::new(Arc::clone(&permissions), None)),
            Box::new(TierStage::new(Arc::clone(&permissions))),
        ]);
        Self {
            _database: database,
            permissions,
            policy,
            admission,
            judge: ExecJudgeDto {
                provider_id: provider,
                model_id: JUDGE_MODEL.to_owned(),
            },
        }
    }

    fn mode(&self, mode: ExecMode) {
        self.policy
            .set(&ExecPolicyDto {
                mode,
                judge: Some(self.judge.clone()),
            })
            .expect("stores the policy");
    }

    fn tier(&self, tool: &str, tier: ToolTier) {
        self.permissions
            .grant(tool, GLOBAL_SCOPE, tier, "test")
            .expect("grants the tier");
    }

    async fn admitted(&self, tool: &str, action: &ReviewedAction) -> Decision {
        let context = StepContext::new(
            RunId::new(new_id()),
            CancellationToken::new(),
            KernelServices::default(),
            Arc::new(Silent),
        );
        let request = AccessRequest::new(
            Actor {
                run_id: context.run_id.to_string(),
                kind: Some(RunKind::Chat),
                origin: AdmissionOrigin::Model,
                listed: true,
                scopes: Vec::new(),
            },
            tool,
            action.clone().into(),
        );
        let admitted = self
            .admission
            .decide(&request, &mut Pass::new(Some(&context)))
            .await
            .expect("admits");
        if admitted.outcome != Outcome::Ask {
            return admitted;
        }
        let reviewed = self
            .policy
            .pipeline()
            .decide(&request, &mut Pass::new(Some(&context)))
            .await
            .expect("reviews");
        let verdict = self
            .policy
            .review(&context, &Effect::from(action.clone()))
            .await
            .expect("reviews through the port");
        assert_eq!(verdict, reviewed.verdict(), "{tool}");
        reviewed
    }
}

type Row = (
    &'static str,
    ToolTier,
    ReviewedAction,
    [(Outcome, StageName); 4],
);

const MODES: [ExecMode; 4] = [
    ExecMode::AlwaysAsk,
    ExecMode::SafetyAsk,
    ExecMode::AutoBypass,
    ExecMode::NoRestrictions,
];

#[tokio::test]
async fn every_cell_of_the_passport_table_holds_end_to_end() {
    use Outcome::{Allow, Ask, Deny};
    use StageName::{Floor, Judge, Mode, Tier};
    let host = Host::new(EMPTY_PACK);
    let rows: Vec<Row> = vec![
        (
            "tier off",
            ToolTier::Off,
            ReviewedAction::Plain,
            [(Deny, Floor); 4],
        ),
        (
            "tier auto",
            ToolTier::Auto,
            ReviewedAction::Plain,
            [(Allow, Tier); 4],
        ),
        (
            "tier ask, not destructive",
            ToolTier::Ask,
            ReviewedAction::Plain,
            [(Ask, Mode), (Allow, Mode), (Allow, Mode), (Allow, Mode)],
        ),
        (
            "fs.apply delete",
            ToolTier::Ask,
            delete("C:/work/a.txt"),
            [(Ask, Mode), (Ask, Mode), (Ask, Judge), (Allow, Mode)],
        ),
        (
            "browser.submit",
            ToolTier::Ask,
            ReviewedAction::Browser,
            [(Ask, Mode), (Ask, Mode), (Ask, Mode), (Allow, Mode)],
        ),
        (
            "shell.run",
            ToolTier::Ask,
            shell(&["git", "status"]),
            [(Ask, Mode), (Ask, Mode), (Ask, Judge), (Allow, Mode)],
        ),
        (
            "declared destructive",
            ToolTier::Ask,
            ReviewedAction::Declared,
            [(Ask, Mode), (Ask, Mode), (Ask, Mode), (Allow, Mode)],
        ),
    ];
    for (index, (name, tier, action, expected)) in rows.into_iter().enumerate() {
        let tool = format!("tool.{index}");
        host.tier(&tool, tier);
        for (mode, (outcome, stage)) in MODES.into_iter().zip(expected) {
            host.mode(mode);
            let decision = host.admitted(&tool, &action).await;
            assert_eq!(
                (decision.outcome, decision.stage),
                (outcome, stage),
                "{name} in {mode}"
            );
            if stage == Judge {
                assert_eq!(decision.trail, vec![Mode, StageName::Rules, Judge]);
            }
        }
    }
}

#[tokio::test]
async fn vendor_rules_decide_before_the_judge_in_auto_bypass() {
    let host = Host::new(VENDOR_PACK);
    host.mode(ExecMode::AutoBypass);
    host.tier("fs.apply", ToolTier::Ask);
    let denied = host
        .admitted("fs.apply", &delete("C:/work/.git/config"))
        .await;
    assert_eq!(
        (denied.outcome, denied.stage),
        (Outcome::Deny, StageName::Rules)
    );
    assert_eq!(denied.trail, vec![StageName::Mode, StageName::Rules]);
    assert_eq!(
        denied.refusal("fs.apply").code,
        AppErrorCode::PermissionDenied
    );
}

#[tokio::test]
async fn an_unlisted_tool_from_the_model_stops_at_the_floor() {
    let host = Host::new(EMPTY_PACK);
    host.tier("fs.read", ToolTier::Auto);
    let request = AccessRequest::new(
        Actor {
            run_id: "run".to_owned(),
            kind: Some(RunKind::Chat),
            origin: AdmissionOrigin::Model,
            listed: false,
            scopes: Vec::new(),
        },
        "fs.read",
        Effect::Plain,
    );
    let decision = host
        .admission
        .decide(&request, &mut Pass::default())
        .await
        .expect("decides");
    assert_eq!(
        (decision.outcome, decision.stage, decision.source.as_deref()),
        (Outcome::Deny, StageName::Floor, Some(FLOOR_UNLISTED))
    );
    let shortcut = AccessRequest::new(
        Actor {
            origin: AdmissionOrigin::Graph,
            ..request.actor.clone()
        },
        "llm.generate",
        Effect::Plain,
    );
    let decision = host
        .admission
        .decide(&shortcut, &mut Pass::default())
        .await
        .expect("decides");
    assert_eq!(
        (decision.outcome, decision.stage, decision.authorisation()),
        (Outcome::Allow, StageName::Floor, None)
    );
}

fn desktop(app: &str) -> Effect {
    Effect::Desktop {
        apps: vec![app.to_owned()],
        sensitive: true,
    }
}

#[test]
fn sensitive_input_into_a_terminal_is_a_process_and_not_a_browser_action() {
    assert_eq!(desktop("pwsh.exe").family(), Family::Process);
    assert!(!desktop("pwsh.exe").reviewable());
    assert_eq!(desktop("regedit.exe").family(), Family::Browser);
    assert!(desktop("regedit.exe").reviewable());
    let typing = Effect::Desktop {
        apps: vec!["cmd.exe".to_owned()],
        sensitive: false,
    };
    assert_eq!(typing.family(), Family::Benign);
}

#[tokio::test]
async fn terminal_input_asks_in_every_mode_but_no_restrictions() {
    let host = Host::new(VENDOR_PACK);
    let context = StepContext::new(
        RunId::new(new_id()),
        CancellationToken::new(),
        KernelServices::default(),
        Arc::new(Silent),
    );
    for mode in MODES {
        host.mode(mode);
        let request = AccessRequest::review(context.run_id.to_string(), &desktop("pwsh.exe"));
        let decision = host
            .policy
            .pipeline()
            .decide(&request, &mut Pass::new(Some(&context)))
            .await
            .expect("decides");
        let verdict = host
            .policy
            .review(&context, &desktop("pwsh.exe"))
            .await
            .expect("reviews");
        if mode == ExecMode::NoRestrictions {
            assert_eq!(decision.outcome, Outcome::Allow, "{mode}");
            assert!(matches!(verdict, ReviewVerdict::Run(_)), "{mode}");
        } else {
            assert_eq!(
                (decision.outcome, decision.stage),
                (Outcome::Ask, StageName::Mode),
                "{mode}"
            );
            assert_eq!(verdict, ReviewVerdict::Ask { reason: None }, "{mode}");
        }
    }
}

struct Answering {
    requests: Mutex<Vec<ApprovalRequestDto>>,
}

impl StepLink for Answering {
    fn emit(&self, _event: HostEventDraft) -> Result<(), AppError> {
        Ok(())
    }

    fn request_approval(
        &self,
        request: ApprovalRequestDto,
    ) -> BoxFuture<'static, Result<ApprovalDecision, AppError>> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        Box::pin(async { Ok(ApprovalDecision::Approved) })
    }
}

#[tokio::test]
async fn a_call_that_always_asks_reaches_the_person_in_no_restrictions() {
    let host = Host::new(VENDOR_PACK);
    host.mode(ExecMode::NoRestrictions);
    host.tier("mail.send", ToolTier::Auto);
    let approvals = Arc::new(
        crate::services::approvals::ApprovalService::new(
            host._database.shared(),
            Arc::clone(&host.permissions),
            std::time::Duration::from_secs(60),
            Arc::new(FakeClock::default()),
        )
        .expect("an approval service"),
    );
    let keeper = super::Gatekeeper::new(&host.permissions, approvals, None);
    let link = Arc::new(Answering {
        requests: Mutex::new(Vec::new()),
    });
    let context = StepContext::new(
        RunId::new(new_id()),
        CancellationToken::new(),
        KernelServices {
            policy: Some(Arc::new(host.policy.clone())),
            ..KernelServices::default()
        },
        link.clone(),
    )
    .recording();
    let remote = Effect::Remote {
        service: "send".to_owned(),
        action: "Отправка письма".to_owned(),
    };
    let mut request = AccessRequest::new(Actor::run(""), "mail.send", remote.clone());
    request.always_ask = true;
    request.prompt.kind = Some("mail".to_owned());
    request.prompt.action = Some("Отправка письма".to_owned());
    request
        .prompt
        .fields
        .push(("account".to_owned(), serde_json::json!("Поддержка")));
    request
        .prompt
        .fields
        .push(("subject".to_owned(), serde_json::json!("Отчёт")));
    drop(
        keeper
            .admit(request, &context)
            .await
            .expect("approved by the person"),
    );
    let asked = link
        .requests
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(asked.len(), 1);
    let keys: Vec<&str> = asked[0].keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "id",
            "runId",
            "subject",
            "scope",
            "expiresAt",
            "kind",
            "tool",
            "action",
            "account"
        ]
    );
    assert_eq!(asked[0]["kind"], "mail");
    assert_eq!(asked[0]["subject"], "mail.send");
    assert_eq!(
        context.authorisation(),
        Some(ToolAuthorisationDto::of(ToolAuthorisationKind::Approved))
    );
    let plain = AccessRequest::new(Actor::run(""), "mail.send", remote);
    drop(
        keeper
            .admit(plain, &context)
            .await
            .expect("an ordinary call runs"),
    );
    assert_eq!(
        link.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len(),
        1
    );
}
