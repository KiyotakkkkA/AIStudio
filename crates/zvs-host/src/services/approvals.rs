use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::json;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, ApprovalRequestDto, ToolTier};

use super::permissions::{Admission, GLOBAL_SCOPE, PermissionContext, PermissionService};
use crate::data::schema::{GRANTED_BY_USER, PendingApprovalRow};
use crate::data::{Database, Repositories};
use crate::kernel::ApprovalDecision;
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::policy::ReviewedAction;

pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);
pub const NO_LONGER_PENDING: &str = "Approval is no longer pending";
pub const PERMISSION_REVOKED: &str = "Permission revoked";
pub const APPROVAL_GONE: &str = "Подтверждение уже не ожидает ответа";
const MAX_TIMEOUT_MS: u128 = 2_147_483_647;

pub struct ApprovalService {
    database: Arc<Database>,
    permissions: Arc<PermissionService>,
    timeout: Duration,
    clock: SharedClock,
    waiting: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
    reviews: Mutex<HashMap<String, PendingReview>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingReview {
    pub run_id: String,
    pub action: ReviewedAction,
    pub conversation: Option<String>,
}

impl ApprovalService {
    pub fn new(
        database: Arc<Database>,
        permissions: Arc<PermissionService>,
        timeout: Duration,
        clock: SharedClock,
    ) -> Result<Self, AppError> {
        let millis = timeout.as_millis();
        if !(1..=MAX_TIMEOUT_MS).contains(&millis)
            || !timeout.subsec_nanos().is_multiple_of(1_000_000)
        {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                "Invalid approval timeout",
            ));
        }
        Ok(Self {
            database,
            permissions,
            timeout,
            clock,
            waiting: Mutex::new(HashMap::new()),
            reviews: Mutex::new(HashMap::new()),
        })
    }

    pub fn create(
        &self,
        run_id: &str,
        subject: &str,
        scope: &str,
    ) -> Result<ApprovalRequestDto, AppError> {
        let id = new_id().hyphenated().to_string();
        let expires_at =
            self.clock.now() + i64::try_from(self.timeout.as_millis()).unwrap_or(i64::MAX);
        self.database.with_connection(|connection| {
            Repositories::new(connection)
                .permissions
                .create_approval(&PendingApprovalRow {
                    id: id.clone(),
                    run_id: run_id.to_owned(),
                    subject: subject.to_owned(),
                    scope: scope.to_owned(),
                    expires_at,
                    decision: None,
                })?;
            Ok(())
        })?;
        Ok(zvs_contract::log_line(&[
            ("id", json!(id)),
            ("runId", json!(run_id)),
            ("subject", json!(subject)),
            ("scope", json!(scope)),
            ("expiresAt", json!(expires_at)),
        ]))
    }

    fn row(&self, id: &str) -> Result<Option<PendingApprovalRow>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).permissions.approval(id)?)
        })
    }

    pub fn remember(&self, id: &str, review: PendingReview) {
        self.reviews
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned(), review);
    }

    fn forget(&self, id: &str) {
        self.reviews
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
    }

    pub fn reviewed(&self, run_id: &str, id: &str) -> Result<PendingReview, AppError> {
        let gone = || AppError::new(AppErrorCode::Conflict, APPROVAL_GONE);
        if !self.is_pending(run_id, id)? {
            self.forget(id);
            return Err(gone());
        }
        self.reviews
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .filter(|review| review.run_id == run_id)
            .cloned()
            .ok_or_else(gone)
    }

    fn settle(&self, id: &str, decision: ApprovalDecision) -> Result<(), AppError> {
        self.forget(id);
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        self.database.with_connection(|connection| {
            Repositories::new(connection)
                .permissions
                .decide(id, decision)?;
            Ok(())
        })
    }

    pub async fn wait(
        &self,
        request: &ApprovalRequestDto,
        cancel: &CancellationToken,
    ) -> Result<ApprovalDecision, AppError> {
        let id = request
            .get("id")
            .map(|value| {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned)
            })
            .unwrap_or_default();
        let Some(row) = self.row(&id)? else {
            self.forget(&id);
            return Ok(ApprovalDecision::Denied);
        };
        if let Some(decision) = row.decision {
            self.forget(&id);
            return Ok(decision);
        }
        let (sender, receiver) = oneshot::channel();
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.clone(), sender);
        let remaining = u64::try_from(row.expires_at - self.clock.now()).unwrap_or(0);
        let decision = tokio::select! {
            biased;
            decided = receiver => decided.unwrap_or(ApprovalDecision::Denied),
            () = cancel.cancelled() => ApprovalDecision::Denied,
            () = tokio::time::sleep(Duration::from_millis(remaining)) => ApprovalDecision::Denied,
        };
        self.settle(&id, decision)?;
        Ok(decision)
    }

    pub fn is_pending(&self, run_id: &str, id: &str) -> Result<bool, AppError> {
        Ok(self.row(id)?.is_some_and(|row| {
            row.run_id == run_id && row.decision.is_none() && row.expires_at > self.clock.now()
        }))
    }

    pub fn decide(
        &self,
        run_id: &str,
        id: &str,
        decision: ApprovalDecision,
        always: bool,
    ) -> Result<(), AppError> {
        self.decide_as(run_id, id, decision, always, GRANTED_BY_USER)
    }

    pub fn decide_as(
        &self,
        run_id: &str,
        id: &str,
        decision: ApprovalDecision,
        always: bool,
        granted_by: &str,
    ) -> Result<(), AppError> {
        let row = self
            .row(id)?
            .filter(|row| {
                row.run_id == run_id && row.decision.is_none() && row.expires_at > self.clock.now()
            })
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_LONGER_PENDING))?;
        let graph_scopes = self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .runs
                .get(run_id)?
                .map(|run| run.graph.permission_scopes().to_vec())
                .unwrap_or_default())
        })?;
        let mut scopes = graph_scopes;
        if row.scope != GLOBAL_SCOPE {
            scopes.push(row.scope.clone());
        }
        if decision == ApprovalDecision::Approved
            && self.permissions.admit_subject(
                &row.subject,
                &PermissionContext {
                    run_id: run_id.to_owned(),
                    scopes,
                },
                ToolTier::Auto,
            )? == Admission::Deny
        {
            return Err(AppError::new(
                AppErrorCode::PermissionDenied,
                PERMISSION_REVOKED,
            ));
        }
        self.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            if always && decision == ApprovalDecision::Approved {
                self.permissions.grant_in(
                    &repositories,
                    &row.subject,
                    &row.scope,
                    ToolTier::Auto,
                    granted_by,
                )?;
            }
            repositories.permissions.decide(id, decision)?;
            Ok(())
        })?;
        self.forget(id);
        let waiter = self
            .waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(decision);
        }
        Ok(())
    }
}
