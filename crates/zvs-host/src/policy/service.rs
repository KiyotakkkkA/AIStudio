use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use zvs_contract::{
    AllowRuleInput, AppError, AppErrorCode, ExecJudgeDto, ExecMode, ExecPolicyDto, PolicyRuleDto,
    ProviderCapability, TOOL_AUTHORISATION_REASON_MAX, TOOL_AUTHORISATION_SOURCE_MAX,
    ToolAuthorisationDto, ToolAuthorisationKind,
};

use super::action::{ReviewVerdict, ReviewedAction};
use super::judge::{JudgeOutcome, ask_judge, judge_prompt};
use super::rules::{RuleOutcome, RuleSet, VENDOR_PACK};
use super::table::{Decision, decide};
use crate::data::{Database, Repositories};
use crate::kernel::{ExecReview, StepContext};
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::settings::SettingService;

pub const EXEC_POLICY_KEY: &str = "exec.policy";
pub const POLICY_DIR: &str = "policy";
pub const JUDGE_CALLS_PER_RUN: u32 = 50;
pub const JUDGE_TIMEOUT: Duration = Duration::from_secs(30);
pub const JUDGE_UNAVAILABLE: &str =
    "Модель-арбитр недоступна: выберите включённого провайдера и доступную модель";
pub const JUDGE_REQUIRED: &str = "Для режима «Автопроверка» нужна модель-арбитр";
pub const RULE_DENIED: &str = "Действие запрещено правилом политики выполнения";
pub const RULES_UNAVAILABLE: &str = "Пользовательские правила недоступны";
const LOG_SOURCE: &str = "policy";

pub struct ExecPolicyOptions {
    pub database: Arc<Database>,
    pub clock: SharedClock,
    pub logger: SharedLogger,
    pub packs: Option<PathBuf>,
    pub vendor: String,
    pub timeout: Duration,
    pub judge_calls: u32,
}

impl ExecPolicyOptions {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self {
            database,
            clock,
            logger: silent_logger(),
            packs: None,
            vendor: VENDOR_PACK.to_owned(),
            timeout: JUDGE_TIMEOUT,
            judge_calls: JUDGE_CALLS_PER_RUN,
        }
    }
}

struct Inner {
    database: Arc<Database>,
    settings: SettingService,
    logger: SharedLogger,
    packs: Option<PathBuf>,
    vendor: String,
    timeout: Duration,
    judge_calls: u32,
    spent: Mutex<HashMap<String, u32>>,
    writing: Mutex<()>,
}

#[derive(Clone)]
pub struct ExecPolicyService {
    inner: Arc<Inner>,
}

impl ExecPolicyService {
    pub fn new(options: ExecPolicyOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                settings: SettingService::new(Arc::clone(&options.database), options.clock),
                database: options.database,
                logger: options.logger,
                packs: options.packs,
                vendor: options.vendor,
                timeout: options.timeout,
                judge_calls: options.judge_calls,
                spent: Mutex::new(HashMap::new()),
                writing: Mutex::new(()),
            }),
        }
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, serde_json::Value)]) {
        self.inner.logger.log(level, LOG_SOURCE, message, fields);
    }

    fn stored(&self) -> Result<ExecPolicyDto, AppError> {
        let Some(setting) = self.inner.settings.get(EXEC_POLICY_KEY)? else {
            return Ok(ExecPolicyDto::default());
        };
        Ok(serde_json::from_value(setting.value).unwrap_or_else(|_| {
            self.log(
                LogLevel::Warn,
                "Stored execution policy is malformed; using always-ask",
                &[],
            );
            ExecPolicyDto::default()
        }))
    }

    fn store(&self, policy: &ExecPolicyDto) -> Result<(), AppError> {
        self.inner.settings.set(EXEC_POLICY_KEY, &json!(policy))?;
        Ok(())
    }

    fn judge_model(&self, judge: &ExecJudgeDto) -> Result<Option<String>, AppError> {
        self.inner.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let Some(provider) = repositories.providers.find_by_id(&judge.provider_id)? else {
                return Ok(None);
            };
            if !provider.enabled || !provider.capabilities.contains(&ProviderCapability::Text) {
                return Ok(None);
            }
            Ok(repositories
                .models
                .list_by_provider(&provider.id, false)?
                .into_iter()
                .find(|model| {
                    model.available
                        && (model.id == judge.model_id || model.external_id == judge.model_id)
                })
                .map(|model| model.external_id))
        })
    }

    fn sanitized(&self, policy: ExecPolicyDto) -> Result<ExecPolicyDto, AppError> {
        let judge = match policy.judge {
            Some(judge) if self.judge_model(&judge)?.is_some() => Some(judge),
            _ => None,
        };
        let mode = if policy.mode == ExecMode::AutoBypass && judge.is_none() {
            ExecMode::SafetyAsk
        } else {
            policy.mode
        };
        Ok(ExecPolicyDto { mode, judge })
    }

    pub fn get(&self) -> Result<ExecPolicyDto, AppError> {
        let stored = self.stored()?;
        let policy = self.sanitized(stored.clone())?;
        if policy != stored {
            self.log(
                LogLevel::Info,
                "The judge model is gone; execution policy adjusted",
                &[("mode", json!(policy.mode))],
            );
            self.store(&policy)?;
        }
        Ok(policy)
    }

    pub fn set(&self, input: &ExecPolicyDto) -> Result<ExecPolicyDto, AppError> {
        if let Some(judge) = &input.judge
            && self.judge_model(judge)?.is_none()
        {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                JUDGE_UNAVAILABLE,
            ));
        }
        if input.mode == ExecMode::AutoBypass && input.judge.is_none() {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                JUDGE_REQUIRED,
            ));
        }
        self.store(input)?;
        Ok(input.clone())
    }

    pub fn allow_rule(&self, input: &AllowRuleInput) -> Result<PolicyRuleDto, AppError> {
        let folder = self
            .inner
            .packs
            .as_deref()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, RULES_UNAVAILABLE))?;
        let _writing = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let written = super::user_rules::allow(folder, input)?;
        self.log(
            LogLevel::Info,
            "A user allow rule was added",
            &[("rule", json!(written.id))],
        );
        Ok(PolicyRuleDto {
            id: written.id,
            text: written.text,
        })
    }

    pub fn rules(&self) -> RuleSet {
        let (rules, problems) = RuleSet::load(&self.inner.vendor, self.inner.packs.as_deref());
        for problem in problems {
            self.log(
                LogLevel::Warn,
                "A policy rule pack was skipped",
                &[("problem", json!(problem))],
            );
        }
        rules
    }

    fn spend(&self, run_id: &str) -> bool {
        let mut spent = self
            .inner
            .spent
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let count = spent.entry(run_id.to_owned()).or_insert(0);
        if *count >= self.inner.judge_calls {
            return false;
        }
        *count += 1;
        true
    }

    async fn judge(
        &self,
        context: &StepContext,
        policy: &ExecPolicyDto,
        action: &ReviewedAction,
        rules: &RuleSet,
    ) -> Result<ReviewVerdict, AppError> {
        let ask = Ok(ReviewVerdict::Ask { reason: None });
        let Some(judge) = &policy.judge else {
            return ask;
        };
        let Some(model) = self.judge_model(judge)? else {
            self.log(LogLevel::Warn, "The judge model is unavailable", &[]);
            return ask;
        };
        let Some(providers) = &context.services.providers else {
            return ask;
        };
        let Ok(driver) = providers.text(&judge.provider_id) else {
            self.log(LogLevel::Warn, "The judge provider has no driver", &[]);
            return ask;
        };
        let run = context.run_id.to_string();
        if !self.spend(&run) {
            self.log(
                LogLevel::Info,
                "The judge call cap for this run is spent",
                &[
                    ("runId", json!(run)),
                    ("cap", json!(self.inner.judge_calls)),
                ],
            );
            return ask;
        }
        let files = matches!(action, ReviewedAction::Files { .. });
        let prompt = judge_prompt(action, &rules.texts(files));
        match ask_judge(&driver, &model, prompt, self.inner.timeout, &context.cancel).await {
            JudgeOutcome::Allow(reason) => Ok(ReviewVerdict::Run(
                ToolAuthorisationDto::of(ToolAuthorisationKind::Judge)
                    .from_source(limited(&model, TOOL_AUTHORISATION_SOURCE_MAX))
                    .because(Some(reason)),
            )),
            JudgeOutcome::Ask(reason) => Ok(ReviewVerdict::Ask {
                reason: Some(reason),
            }),
            JudgeOutcome::Failed(failure) => {
                context.check_cancelled()?;
                self.log(
                    LogLevel::Warn,
                    "The judge gave no verdict; asking the user",
                    &[("failure", json!(failure.as_str()))],
                );
                ask
            }
        }
    }
}

#[async_trait]
impl ExecReview for ExecPolicyService {
    async fn review(
        &self,
        context: &StepContext,
        action: &ReviewedAction,
    ) -> Result<ReviewVerdict, AppError> {
        let policy = self.get()?;
        match decide(policy.mode, action.family()) {
            Decision::Run => Ok(ReviewVerdict::Run(
                ToolAuthorisationDto::of(ToolAuthorisationKind::Policy)
                    .from_source(policy.mode.as_str()),
            )),
            Decision::Ask => Ok(ReviewVerdict::Ask { reason: None }),
            Decision::Deny => Ok(ReviewVerdict::Deny {
                rule: "floor".to_owned(),
            }),
            Decision::Review => {
                let rules = self.rules();
                match rules.evaluate(action) {
                    RuleOutcome::Deny { rule } => Ok(ReviewVerdict::Deny { rule }),
                    RuleOutcome::Ask { text, .. } => Ok(ReviewVerdict::Ask {
                        reason: Some(limited(&text, TOOL_AUTHORISATION_REASON_MAX)),
                    }),
                    RuleOutcome::Allow { rule, text } => Ok(ReviewVerdict::Run(
                        ToolAuthorisationDto::of(ToolAuthorisationKind::Judge)
                            .from_source(limited(&rule, TOOL_AUTHORISATION_SOURCE_MAX))
                            .because(Some(limited(&text, TOOL_AUTHORISATION_REASON_MAX))),
                    )),
                    RuleOutcome::Unmatched => self.judge(context, &policy, action, &rules).await,
                }
            }
        }
    }
}

fn limited(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub fn rule_denied(rule: &str) -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, RULE_DENIED).with_detail("rule", rule)
}
