use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{AppError, AppErrorCode, ToolAuthorisationDto, ToolAuthorisationKind};

use super::admitted::Clearance;
use super::decision::{Decision, Outcome, StageName};
use super::person::{ask_grant, ask_tool};
use super::pipeline::{AccessPipeline, Pass};
use super::request::{AccessRequest, Cleared, Effect, Gating, Resource};
use super::stages::{FloorStage, TierStage};
use crate::kernel::StepContext;
use crate::policy::{ReviewVerdict, rule_denied};
use crate::services::approvals::ApprovalService;
use crate::services::permissions::PermissionService;
use crate::tools::ToolRegistry;

pub const PERMISSION_REVOKED: &str = "Permission revoked";

#[async_trait]
pub trait AccessGate: Send + Sync {
    async fn screen(&self, request: &AccessRequest, context: &StepContext) -> Result<(), AppError>;

    async fn admit(
        &self,
        request: AccessRequest,
        context: &StepContext,
    ) -> Result<Clearance, AppError>;
}

pub type SharedGate = Arc<dyn AccessGate>;

pub async fn review(context: &StepContext, effect: &Effect) -> Result<ReviewVerdict, AppError> {
    match &context.services.policy {
        Some(policy) => policy.review(context, effect).await,
        None => Ok(ReviewVerdict::Ask { reason: None }),
    }
}

pub struct Gatekeeper {
    admission: AccessPipeline,
    folder: AccessPipeline,
    screening: AccessPipeline,
    approvals: Arc<ApprovalService>,
}

impl Gatekeeper {
    pub fn new(
        permissions: &Arc<PermissionService>,
        approvals: Arc<ApprovalService>,
        tools: Option<&ToolRegistry>,
    ) -> Self {
        let floor = || FloorStage::new(Arc::clone(permissions), tools.cloned());
        Self {
            admission: AccessPipeline::new(vec![
                Box::new(floor()),
                Box::new(TierStage::new(Arc::clone(permissions))),
            ]),
            folder: AccessPipeline::new(vec![Box::new(floor().without_shortcuts())]),
            screening: AccessPipeline::new(vec![Box::new(floor())]),
            approvals,
        }
    }

    pub const fn approvals(&self) -> &Arc<ApprovalService> {
        &self.approvals
    }

    fn cleared(request: &AccessRequest, by: Cleared) {
        if let Some(attendant) = &request.prompt.attendant {
            attendant.cleared(by);
        }
    }

    pub async fn screen(
        &self,
        request: &AccessRequest,
        context: &StepContext,
    ) -> Result<(), AppError> {
        let pipeline = match request.gating {
            Gating::Open => return Ok(()),
            Gating::Tool => &self.screening,
            Gating::AppFolder => &self.folder,
        };
        let decision = pipeline
            .decide(request, &mut Pass::new(Some(context)))
            .await?;
        if decision.outcome == Outcome::Deny {
            return Err(decision.refusal(&request.tool));
        }
        Ok(())
    }

    pub async fn admit(
        &self,
        mut request: AccessRequest,
        context: &StepContext,
    ) -> Result<Clearance, AppError> {
        context.check_cancelled()?;
        if request.actor.run_id.is_empty() {
            request.actor.run_id = context.run_id.to_string();
        }
        match request.gating {
            Gating::Open => {
                ask_grant(&self.approvals, context, &request, false).await?;
                return Ok(Clearance::new());
            }
            Gating::AppFolder => {
                let decision = self
                    .folder
                    .decide(&request, &mut Pass::new(Some(context)))
                    .await?;
                if decision.outcome == Outcome::Deny {
                    return Err(decision.refusal(&request.tool));
                }
                context.authorise(ToolAuthorisationDto::of(ToolAuthorisationKind::Auto))?;
                Self::cleared(&request, Cleared::Admission);
                ask_grant(&self.approvals, context, &request, false).await?;
                return Ok(Clearance::new());
            }
            Gating::Tool => {}
        }
        let decision = self
            .admission
            .decide(&request, &mut Pass::new(Some(context)))
            .await?;
        let mut asking = match decision.outcome {
            Outcome::Deny => return Err(decision.refusal(&request.tool)),
            Outcome::Ask => true,
            Outcome::Allow => request.always_ask,
        };
        if !asking {
            if let Some(authorisation) = decision.authorisation() {
                context.authorise(authorisation)?;
            }
            Self::cleared(&request, Cleared::Admission);
        }
        let mut reason = None;
        if asking && !request.always_ask {
            match review(context, &request.effect).await? {
                ReviewVerdict::Run(authorisation) => {
                    context.authorise(authorisation)?;
                    Self::cleared(&request, Cleared::Review);
                    asking = false;
                }
                ReviewVerdict::Deny { rule } => return Err(rule_denied(&rule)),
                ReviewVerdict::Ask { reason: why } => reason = why,
            }
        }
        let merged = asking
            && matches!(request.effect, Effect::Files { .. })
            && request
                .resources
                .iter()
                .any(|resource| matches!(resource, Resource::Folder { .. }));
        ask_grant(&self.approvals, context, &request, merged).await?;
        if asking && !merged {
            ask_tool(&self.approvals, context, &request).await?;
            let recheck = self
                .admission
                .decide(&request, &mut Pass::new(Some(context)))
                .await?;
            if recheck.outcome == Outcome::Deny {
                return Err(AppError::new(
                    AppErrorCode::PermissionDenied,
                    PERMISSION_REVOKED,
                ));
            }
        }
        if asking {
            context.authorise(
                Decision::allow(StageName::Person)
                    .because(reason)
                    .authorisation()
                    .unwrap_or_else(|| ToolAuthorisationDto::of(ToolAuthorisationKind::Approved)),
            )?;
            Self::cleared(&request, Cleared::Person);
        }
        Ok(Clearance::new())
    }
}

#[async_trait]
impl AccessGate for Gatekeeper {
    async fn screen(&self, request: &AccessRequest, context: &StepContext) -> Result<(), AppError> {
        Self::screen(self, request, context).await
    }

    async fn admit(
        &self,
        request: AccessRequest,
        context: &StepContext,
    ) -> Result<Clearance, AppError> {
        Self::admit(self, request, context).await
    }
}

#[cfg(test)]
pub struct OpenGate;

#[cfg(test)]
#[async_trait]
impl AccessGate for OpenGate {
    async fn screen(
        &self,
        _request: &AccessRequest,
        _context: &StepContext,
    ) -> Result<(), AppError> {
        Ok(())
    }

    async fn admit(
        &self,
        _request: AccessRequest,
        _context: &StepContext,
    ) -> Result<Clearance, AppError> {
        Ok(Clearance::new())
    }
}
