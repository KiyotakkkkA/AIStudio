use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{
    AppError, RunKind, TOOL_AUTHORISATION_REASON_MAX, TOOL_AUTHORISATION_SOURCE_MAX,
};

use super::decision::{Decision, StageName};
use super::pipeline::{Pass, Stage, Step};
use super::request::AccessRequest;
use crate::kernel::AdmissionOrigin;
use crate::policy::rules::RuleText;
use crate::policy::service::limited;
use crate::policy::{
    Decision as ModeDecision, ExecPolicyService, ReviewVerdict, RuleEffect, RuleOutcome, decide,
};
use crate::services::permissions::{Admission, PermissionContext, PermissionService};
use crate::tools::ToolRegistry;

pub const OUTSIDE_ALLOWED_TOOLS: &str = "Инструмент не может быть разрешён в текущем запуске";
pub const TOOL_DISABLED: &str = "Tool disabled";
pub const PERMISSION_DENIED: &str = "Permission denied";
pub const FLOOR_UNLISTED: &str = "unlisted";
pub const FLOOR_DISABLED: &str = "disabled";
pub const FLOOR_OFF: &str = "off";
pub const FLOOR_SHORTCUT: &str = "graph";
pub const TIER_GRANTED: &str = "granted";
pub const TIER_AUTO: &str = "auto";

fn permission_context(request: &AccessRequest) -> PermissionContext {
    PermissionContext {
        run_id: request.actor.run_id.clone(),
        scopes: request.actor.scopes.clone(),
    }
}

fn graph_shortcut(kind: Option<RunKind>, tool: &str) -> bool {
    matches!(
        (kind, tool),
        (
            Some(RunKind::Chat | RunKind::Skill | RunKind::Scenario | RunKind::Agentic),
            "llm.generate"
        ) | (Some(RunKind::Chat), "vector.search")
            | (Some(RunKind::Indexing), "vector.index")
            | (Some(RunKind::Download), "download.fetch")
    )
}

pub struct FloorStage {
    permissions: Arc<PermissionService>,
    tools: Option<ToolRegistry>,
    shortcuts: bool,
}

impl FloorStage {
    pub const fn new(permissions: Arc<PermissionService>, tools: Option<ToolRegistry>) -> Self {
        Self {
            permissions,
            tools,
            shortcuts: true,
        }
    }

    #[must_use]
    pub const fn without_shortcuts(mut self) -> Self {
        self.shortcuts = false;
        self
    }

    fn disabled(&self, tool: &str) -> Result<bool, AppError> {
        let Some(tools) = &self.tools else {
            return Ok(false);
        };
        Ok(tools
            .ceiling(tool)?
            .is_some_and(|ceiling| !ceiling.enabled || !ceiling.available))
    }
}

#[async_trait]
impl Stage for FloorStage {
    fn name(&self) -> StageName {
        StageName::Floor
    }

    fn fails_closed(&self) -> bool {
        true
    }

    async fn check(&self, request: &AccessRequest, _pass: &mut Pass<'_>) -> Result<Step, AppError> {
        let actor = &request.actor;
        let deny = |source| {
            Ok(Step::Decide(
                Decision::deny(StageName::Floor).from_source(source),
            ))
        };
        if actor.origin == AdmissionOrigin::Model && !actor.listed {
            return deny(FLOOR_UNLISTED);
        }
        if self.shortcuts
            && actor.origin == AdmissionOrigin::Graph
            && graph_shortcut(actor.kind, &request.tool)
        {
            return Ok(Step::Decide(
                Decision::allow(StageName::Floor).from_source(FLOOR_SHORTCUT),
            ));
        }
        if self.disabled(&request.tool)? {
            return deny(FLOOR_DISABLED);
        }
        if self
            .permissions
            .refused(&request.tool, &permission_context(request))?
        {
            return deny(FLOOR_OFF);
        }
        Ok(Step::Continue)
    }
}

pub struct TierStage {
    permissions: Arc<PermissionService>,
}

impl TierStage {
    pub const fn new(permissions: Arc<PermissionService>) -> Self {
        Self { permissions }
    }
}

#[async_trait]
impl Stage for TierStage {
    fn name(&self) -> StageName {
        StageName::Tier
    }

    fn fails_closed(&self) -> bool {
        true
    }

    async fn check(&self, request: &AccessRequest, _pass: &mut Pass<'_>) -> Result<Step, AppError> {
        let context = permission_context(request);
        let decision =
            match self
                .permissions
                .admit_subject(&request.tool, &context, request.minimum)?
            {
                Admission::Allow => {
                    let source = if self.permissions.granted(&request.tool, &context)? {
                        TIER_GRANTED
                    } else {
                        TIER_AUTO
                    };
                    Decision::allow(StageName::Tier).from_source(source)
                }
                Admission::Ask => Decision::ask(StageName::Tier),
                Admission::Deny => Decision::deny(StageName::Tier),
            };
        Ok(Step::Decide(decision))
    }
}

pub struct HardRulesStage {
    policy: ExecPolicyService,
}

impl HardRulesStage {
    pub const fn new(policy: ExecPolicyService) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl Stage for HardRulesStage {
    fn name(&self) -> StageName {
        StageName::HardRules
    }

    fn fails_closed(&self) -> bool {
        true
    }

    async fn check(&self, request: &AccessRequest, pass: &mut Pass<'_>) -> Result<Step, AppError> {
        let rules = self.policy.rules();
        let denied = rules.denied(pass.action(request));
        pass.rules = Some(rules);
        Ok(match denied {
            Some(rule) => Step::Decide(Decision::deny(StageName::HardRules).from_source(rule)),
            None => Step::Continue,
        })
    }
}

pub struct ModeStage {
    policy: ExecPolicyService,
}

impl ModeStage {
    pub const fn new(policy: ExecPolicyService) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl Stage for ModeStage {
    fn name(&self) -> StageName {
        StageName::Mode
    }

    async fn check(&self, request: &AccessRequest, pass: &mut Pass<'_>) -> Result<Step, AppError> {
        let policy = self.policy.get()?;
        let mode = policy.mode;
        pass.policy = Some(policy);
        Ok(match decide(mode, request.effect.family()) {
            ModeDecision::Run => {
                Step::Decide(Decision::allow(StageName::Mode).from_source(mode.as_str()))
            }
            ModeDecision::Ask => Step::Decide(Decision::ask(StageName::Mode)),
            ModeDecision::Rules | ModeDecision::Review if !request.effect.reviewable() => {
                Step::Decide(Decision::ask(StageName::Mode))
            }
            ModeDecision::Rules => {
                pass.rules_only = true;
                Step::Continue
            }
            ModeDecision::Review => Step::Continue,
        })
    }
}

pub struct RulesStage {
    policy: ExecPolicyService,
}

impl RulesStage {
    pub const fn new(policy: ExecPolicyService) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl Stage for RulesStage {
    fn name(&self) -> StageName {
        StageName::Rules
    }

    async fn check(&self, request: &AccessRequest, pass: &mut Pass<'_>) -> Result<Step, AppError> {
        let rules = match pass.rules.take() {
            Some(rules) => rules,
            None => self.policy.rules(),
        };
        let outcome = rules.evaluate(pass.action(request));
        pass.rules = Some(rules);
        Ok(match outcome {
            RuleOutcome::Deny { rule } => {
                Step::Decide(Decision::deny(StageName::Rules).from_source(rule))
            }
            RuleOutcome::Review { text, .. } if pass.rules_only => Step::Decide(
                Decision::ask(StageName::Rules)
                    .because(Some(limited(&text, TOOL_AUTHORISATION_REASON_MAX))),
            ),
            RuleOutcome::Unmatched if pass.rules_only => {
                Step::Decide(Decision::ask(StageName::Rules))
            }
            RuleOutcome::Ask { text, .. } => Step::Decide(
                Decision::ask(StageName::Rules)
                    .because(Some(limited(&text, TOOL_AUTHORISATION_REASON_MAX))),
            ),
            RuleOutcome::Allow { rule, text } => Step::Decide(
                Decision::allow(StageName::Rules)
                    .from_source(limited(&rule, TOOL_AUTHORISATION_SOURCE_MAX))
                    .because(Some(limited(&text, TOOL_AUTHORISATION_REASON_MAX))),
            ),
            RuleOutcome::Review { rule, text } => {
                pass.flagged = Some(RuleText {
                    id: rule,
                    effect: RuleEffect::Review,
                    text,
                });
                Step::Continue
            }
            RuleOutcome::Unmatched => Step::Continue,
        })
    }
}

pub struct JudgeStage {
    policy: ExecPolicyService,
}

impl JudgeStage {
    pub const fn new(policy: ExecPolicyService) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl Stage for JudgeStage {
    fn name(&self) -> StageName {
        StageName::Judge
    }

    async fn check(&self, request: &AccessRequest, pass: &mut Pass<'_>) -> Result<Step, AppError> {
        let flagged = pass.flagged.take();
        let Some(context) = pass.step else {
            return Ok(Step::Decide(Decision::ask(StageName::Judge).because(
                flagged.map(|rule| limited(&rule.text, TOOL_AUTHORISATION_REASON_MAX)),
            )));
        };
        let policy = match pass.policy.take() {
            Some(policy) => policy,
            None => self.policy.get()?,
        };
        if policy.judge.is_none() {
            return Ok(Step::Decide(Decision::ask(StageName::Person).because(
                flagged.map(|rule| limited(&rule.text, TOOL_AUTHORISATION_REASON_MAX)),
            )));
        }
        let rules = match pass.rules.take() {
            Some(rules) => rules,
            None => self.policy.rules(),
        };
        let verdict = self
            .policy
            .judge(
                context,
                &policy,
                pass.action(request),
                &rules,
                flagged.as_ref(),
            )
            .await?;
        let decision = match verdict {
            ReviewVerdict::Run(authorisation) => {
                let decision = Decision::allow(StageName::Judge).because(authorisation.reason);
                match authorisation.source {
                    Some(source) => decision.from_source(source),
                    None => decision,
                }
            }
            ReviewVerdict::Ask { reason } => Decision::ask(StageName::Judge).because(reason),
            ReviewVerdict::Deny { rule } => Decision::deny(StageName::Judge).from_source(rule),
        };
        Ok(Step::Decide(decision))
    }
}
