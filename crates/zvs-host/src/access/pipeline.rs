use std::borrow::Cow;

use async_trait::async_trait;
use serde_json::json;
use zvs_contract::{AppError, AppErrorCode, ExecPolicyDto};

use super::decision::{Decision, StageName};
use super::request::AccessRequest;
use crate::kernel::StepContext;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::policy::{ReviewedAction, RuleSet, rules::RuleText};

pub const STAGE_FAILED: &str = "error";
const LOG_SOURCE: &str = "access";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Continue,
    Decide(Decision),
}

#[derive(Default)]
pub struct Pass<'a> {
    pub step: Option<&'a StepContext>,
    pub policy: Option<ExecPolicyDto>,
    pub rules: Option<RuleSet>,
    pub action: Option<Cow<'a, ReviewedAction>>,
    pub flagged: Option<RuleText>,
}

impl<'a> Pass<'a> {
    pub fn new(step: Option<&'a StepContext>) -> Self {
        Self {
            step,
            ..Self::default()
        }
    }

    pub fn reviewing(mut self, action: &'a ReviewedAction) -> Self {
        self.action = Some(Cow::Borrowed(action));
        self
    }

    pub fn action(&mut self, request: &AccessRequest) -> &ReviewedAction {
        self.action
            .get_or_insert_with(|| Cow::Owned(request.effect.reviewed()))
    }
}

#[async_trait]
pub trait Stage: Send + Sync {
    fn name(&self) -> StageName;

    fn fails_closed(&self) -> bool {
        false
    }

    async fn check(&self, request: &AccessRequest, pass: &mut Pass<'_>) -> Result<Step, AppError>;
}

pub struct AccessPipeline {
    stages: Vec<Box<dyn Stage>>,
    logger: SharedLogger,
}

impl AccessPipeline {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self {
            stages,
            logger: silent_logger(),
        }
    }

    #[must_use]
    pub fn with_logger(mut self, logger: SharedLogger) -> Self {
        self.logger = logger;
        self
    }

    pub fn stages(&self) -> Vec<StageName> {
        self.stages.iter().map(|stage| stage.name()).collect()
    }

    pub async fn decide(
        &self,
        request: &AccessRequest,
        pass: &mut Pass<'_>,
    ) -> Result<Decision, AppError> {
        let mut trail = Vec::with_capacity(self.stages.len());
        for stage in &self.stages {
            let name = stage.name();
            trail.push(name);
            match stage.check(request, pass).await {
                Ok(Step::Continue) => {}
                Ok(Step::Decide(decision)) => return Ok(decision.with_trail(trail)),
                Err(error) if stage.fails_closed() || error.code == AppErrorCode::RunCancelled => {
                    return Err(error);
                }
                Err(error) => {
                    self.logger.log(
                        LogLevel::Warn,
                        LOG_SOURCE,
                        "An access stage failed; asking the user",
                        &[
                            ("stage", json!(name.as_str())),
                            ("tool", json!(request.tool)),
                            ("code", json!(error.code)),
                        ],
                    );
                    return Ok(Decision::ask(name)
                        .from_source(STAGE_FAILED)
                        .with_trail(trail));
                }
            }
        }
        Ok(Decision::ask(StageName::Person).with_trail(trail))
    }
}
