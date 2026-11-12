use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, ConversationId, FolderAccessAnswer,
    FolderGrantChatDto, FolderGrantDto, FolderGrantId, FolderGrantScope, FolderRequestEntryDto,
    RunId, WorkingFolderAccess,
};

use super::paths::{
    ForbiddenFolders, GrantTarget, TargetKind, canonical, covers, grant_target, relative_to,
    resolve, same_folder,
};
use crate::access::{AccessGrants, AccessRequest, Asking, Attendant, GrantQuery, Resource};
use crate::data::schema::{ConversationRow, FolderGrantRow, GRANTED_BY_USER, GrantKind};
use crate::data::{DataError, Database, Repositories};
use crate::kernel::{ApprovalDecision, StepContext};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::services::approvals::{ApprovalService, NO_LONGER_PENDING};

pub use crate::access::FOLDER_SUBJECT;
pub const FOLDER_FORBIDDEN: &str = "Папка недоступна: системные папки, корни дисков, сетевые и съёмные диски и данные приложения закрыты для доступа";
pub const FOLDER_ACCESS_DENIED: &str = "Доступ к папке не предоставлен";
pub const FOLDER_NOT_FOUND: &str = "Папка не найдена";
pub const NOT_A_FOLDER: &str = "Путь не является папкой";
pub const INVALID_PATH: &str = "Некорректный путь";
pub const GRANT_NOT_FOUND: &str = "Разрешение не найдено";
pub const NO_WORKING_FOLDER: &str =
    "Относительный путь нельзя разрешить: у диалога нет рабочей папки";
pub const CONVERSATION_MISSING: &str = "Диалог не найден";
pub const CHANGES_DENIED: &str = "Изменения файлов отклонены";
pub const OUTSIDE_WORKSPACE: &str = "Путь находится вне рабочей папки сессии";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTarget {
    pub path: PathBuf,
    pub kind: TargetKind,
}

impl FolderTarget {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: TargetKind::File,
        }
    }

    pub fn folder(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: TargetKind::Folder,
        }
    }
}

pub struct AccessPlan {
    pub resolved: Vec<GrantTarget>,
    pub folders: Vec<PathBuf>,
    used: Vec<String>,
}

impl AccessPlan {
    pub const fn empty() -> Self {
        Self {
            resolved: Vec::new(),
            folders: Vec::new(),
            used: Vec::new(),
        }
    }
}

pub struct FolderCall {
    conversation_id: String,
    tool: String,
    once: Arc<Mutex<Vec<PathBuf>>>,
}

impl FolderCall {
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    fn once(&self) -> Vec<PathBuf> {
        self.once
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn hold(once: &Mutex<Vec<PathBuf>>, folders: &[PathBuf]) {
    once.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .extend(folders.iter().cloned());
}

pub struct FolderAccess {
    pub resolved: Vec<GrantTarget>,
    folders: Vec<PathBuf>,
    entries: Vec<FolderRequestEntryDto>,
    used: Vec<String>,
    conversation_id: String,
    tool: String,
    attendant: Arc<FolderAttendant>,
    service: FolderAccessService,
}

impl FolderAccess {
    pub fn needs_grant(&self) -> bool {
        !self.folders.is_empty()
    }

    pub fn resources(&self) -> Vec<Resource> {
        self.entries
            .iter()
            .map(|entry| Resource::Folder {
                path: entry.path.clone(),
                creates: entry.creates.clone(),
            })
            .collect()
    }

    pub fn describe(&self, mut request: AccessRequest, action: &str) -> AccessRequest {
        request.prompt.conversation = Some(self.conversation_id.clone());
        request.prompt.action = Some(action.to_owned());
        if self.needs_grant() {
            request.resources.extend(self.resources());
            request.prompt.attendant = Some(Arc::clone(&self.attendant) as Arc<dyn Attendant>);
        }
        request
    }

    pub fn request(&self, action: &str) -> AccessRequest {
        self.describe(AccessRequest::grants(self.tool.clone(), Vec::new()), action)
    }

    pub fn touch(&self) -> Result<(), AppError> {
        if self.used.is_empty() {
            return Ok(());
        }
        let now = self.service.inner.clock.now();
        self.service
            .read(|repositories| Ok(repositories.folder_grants.touch(&self.used, now)?))
    }
}

struct FolderAttendant {
    service: FolderAccessService,
    conversation_id: String,
    folders: Vec<PathBuf>,
    entries: Vec<FolderRequestEntryDto>,
    once: Arc<Mutex<Vec<PathBuf>>>,
}

fn approval_id(approval: &ApprovalRequestDto) -> String {
    approval
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

impl Attendant for FolderAttendant {
    fn asking(&self, asking: Asking, approval: &ApprovalRequestDto) {
        if asking != Asking::Grant {
            return;
        }
        let run_id = approval
            .get("runId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.service.pending().insert(
            approval_id(approval),
            Pending {
                run_id,
                conversation_id: self.conversation_id.clone(),
                folders: self.folders.clone(),
                answer: None,
            },
        );
    }

    fn answered(
        &self,
        asking: Asking,
        approval: &ApprovalRequestDto,
        decision: Result<ApprovalDecision, AppError>,
    ) -> Result<ApprovalDecision, AppError> {
        if asking != Asking::Grant {
            return decision;
        }
        let answer = self
            .service
            .pending()
            .remove(&approval_id(approval))
            .and_then(|pending| pending.answer);
        match (decision?, answer) {
            (ApprovalDecision::Approved, Some(answer)) if answer != FolderAccessAnswer::Deny => {
                hold(&self.once, &self.folders);
                Ok(ApprovalDecision::Approved)
            }
            _ => Err(
                AppError::new(AppErrorCode::PermissionDenied, FOLDER_ACCESS_DENIED)
                    .with_detail("folders", json!(self.entries)),
            ),
        }
    }
}

enum Owner {
    Conversation(Option<PathBuf>),
    Session(PathBuf),
}

struct Pending {
    run_id: String,
    conversation_id: String,
    folders: Vec<PathBuf>,
    answer: Option<FolderAccessAnswer>,
}

struct Inner {
    database: Arc<Database>,
    approvals: Arc<ApprovalService>,
    forbidden: ForbiddenFolders,
    clock: SharedClock,
    grants: AccessGrants,
    pending: Mutex<HashMap<String, Pending>>,
}

#[derive(Clone)]
pub struct FolderAccessService {
    inner: Arc<Inner>,
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn denied(message: &str, path: &Path) -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, message).with_detail("path", path_text(path))
}

fn io_error(path: &Path, error: &std::io::Error) -> AppError {
    let (code, message) = match error.kind() {
        std::io::ErrorKind::NotFound => (AppErrorCode::NotFound, FOLDER_NOT_FOUND),
        std::io::ErrorKind::NotADirectory => (AppErrorCode::ValidationFailed, NOT_A_FOLDER),
        _ => (AppErrorCode::ValidationFailed, INVALID_PATH),
    };
    AppError::new(code, message).with_detail("path", path_text(path))
}

pub fn folder_access(
    folder: Option<&str>,
    grants: &[FolderGrantRow],
    conversation_id: &str,
) -> Option<WorkingFolderAccess> {
    folder.map(|folder| {
        let covered = grants.iter().any(|grant| {
            (grant.scope == FolderGrantScope::Always
                || grant.conversation_id.as_deref() == Some(conversation_id))
                && covers(Path::new(&grant.path), Path::new(folder))
        });
        if covered {
            WorkingFolderAccess::Granted
        } else {
            WorkingFolderAccess::Missing
        }
    })
}

fn minimal(folders: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for folder in folders {
        if kept.iter().any(|known| covers(known, &folder)) {
            continue;
        }
        kept.retain(|known| !covers(&folder, known));
        kept.push(folder);
    }
    kept
}

impl FolderAccessService {
    pub fn new(
        database: Arc<Database>,
        approvals: Arc<ApprovalService>,
        forbidden: ForbiddenFolders,
        clock: SharedClock,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                grants: AccessGrants::new(Arc::clone(&database), clock.clone()),
                database,
                approvals,
                forbidden,
                clock,
                pending: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn read<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        self.inner
            .database
            .with_connection(|connection| work(&Repositories::new(connection)))
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn refuses(&self, folder: &Path) -> bool {
        self.inner.forbidden.refuses(folder)
    }

    pub fn call(&self, conversation_id: &str, tool: &str) -> FolderCall {
        FolderCall {
            conversation_id: conversation_id.to_owned(),
            tool: tool.to_owned(),
            once: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn working_folder(&self, path: &Path) -> Result<PathBuf, AppError> {
        if !path.is_absolute() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, INVALID_PATH)
                .with_detail("path", path_text(path)));
        }
        let folder = canonical(path).map_err(|error| io_error(path, &error))?;
        if !folder.is_dir() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_FOLDER)
                .with_detail("path", path_text(path)));
        }
        if self.refuses(&folder) {
            return Err(denied(FOLDER_FORBIDDEN, &folder));
        }
        Ok(folder)
    }

    fn covering(&self, conversation_id: &str, folder: &Path) -> Result<bool, AppError> {
        let path = path_text(folder);
        Ok(self
            .inner
            .grants
            .covers(&GrantQuery::new(GrantKind::Folder, &path).owner(Some(conversation_id)))?
            .is_some())
    }

    fn owner(&self, id: &str) -> Result<Owner, AppError> {
        let (conversation, workspace) = self.read(|repositories| {
            Ok((
                repositories.chat.get(id)?,
                repositories.agentic_sessions.workspace(id)?,
            ))
        })?;
        match (conversation, workspace) {
            (Some(conversation), _) => Ok(Owner::Conversation(
                conversation.working_folder.map(PathBuf::from),
            )),
            (None, Some(workspace)) => Ok(Owner::Session(PathBuf::from(workspace))),
            (None, None) => Err(AppError::new(AppErrorCode::NotFound, CONVERSATION_MISSING)),
        }
    }

    pub fn working_folder_of(&self, conversation_id: &str) -> Result<Option<PathBuf>, AppError> {
        if conversation_id.is_empty() {
            return Ok(None);
        }
        Ok(match self.owner(conversation_id)? {
            Owner::Conversation(folder) => folder,
            Owner::Session(workspace) => Some(workspace),
        })
    }

    pub fn covered(&self, conversation_id: &str, folder: &Path) -> Result<bool, AppError> {
        if let Owner::Session(workspace) = self.owner(conversation_id)? {
            let workspace = canonical(&workspace).unwrap_or(workspace);
            return Ok(covers(&workspace, folder));
        }
        self.covering(conversation_id, folder)
    }

    pub fn grant(
        &self,
        folder: &Path,
        scope: FolderGrantScope,
        conversation_id: Option<&str>,
    ) -> Result<FolderGrantRow, AppError> {
        self.stored_grant(folder, scope, conversation_id, GRANTED_BY_USER)
            .map(|(grant, _)| grant)
    }

    fn stored_grant(
        &self,
        folder: &Path,
        scope: FolderGrantScope,
        conversation_id: Option<&str>,
        granted_by: &str,
    ) -> Result<(FolderGrantRow, bool), AppError> {
        if self.refuses(folder) {
            return Err(denied(FOLDER_FORBIDDEN, folder));
        }
        let conversation_id = match scope {
            FolderGrantScope::Always => None,
            FolderGrantScope::Conversation => Some(
                conversation_id
                    .ok_or_else(|| {
                        AppError::new(AppErrorCode::ValidationFailed, CONVERSATION_MISSING)
                    })?
                    .to_owned(),
            ),
        };
        let now = self.inner.clock.now();
        self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            if let Some(existing) = repositories
                .folder_grants
                .list()?
                .into_iter()
                .find(|grant| {
                    grant.scope == scope
                        && grant.conversation_id == conversation_id
                        && same_folder(Path::new(&grant.path), folder)
                })
            {
                return Ok((existing, false));
            }
            let created = repositories.folder_grants.create_by(
                &FolderGrantRow {
                    id: new_id().hyphenated().to_string(),
                    path: path_text(folder),
                    scope,
                    conversation_id: conversation_id.clone(),
                    granted_at: now,
                    last_used_at: None,
                },
                granted_by,
            )?;
            Ok((created, true))
        })
    }

    pub fn revoke(&self, id: &FolderGrantId) -> Result<(), AppError> {
        if self.read(|repositories| Ok(repositories.folder_grants.remove(&id.to_string())?))? {
            Ok(())
        } else {
            Err(AppError::new(AppErrorCode::NotFound, GRANT_NOT_FOUND))
        }
    }

    pub fn grants(&self) -> Result<Vec<FolderGrantRow>, AppError> {
        self.read(|repositories| Ok(repositories.folder_grants.list()?))
    }

    pub fn list(&self) -> Result<Vec<FolderGrantDto>, AppError> {
        let (grants, conversations) = self.read(|repositories| {
            Ok((
                repositories.folder_grants.list()?,
                repositories.chat.list()?,
            ))
        })?;
        let chat = |row: &ConversationRow| {
            ConversationId::parse(&row.id).map(|id| FolderGrantChatDto {
                id,
                title: row.title.clone(),
            })
        };
        grants
            .into_iter()
            .map(|grant| {
                let owner = grant.conversation_id.as_deref().and_then(|owner| {
                    conversations
                        .iter()
                        .find(|row| row.id == owner)
                        .and_then(chat)
                });
                let bound_conversations = conversations
                    .iter()
                    .filter(|row| {
                        grant
                            .conversation_id
                            .as_deref()
                            .is_none_or(|owner| owner == row.id)
                    })
                    .filter(|row| {
                        row.working_folder
                            .as_deref()
                            .is_some_and(|folder| covers(Path::new(&grant.path), Path::new(folder)))
                    })
                    .filter_map(chat)
                    .collect();
                Ok(FolderGrantDto {
                    id: FolderGrantId::parse(&grant.id)
                        .ok_or_else(|| AppError::new(AppErrorCode::Unknown, GRANT_NOT_FOUND))?,
                    path: grant.path,
                    scope: grant.scope,
                    conversation: owner,
                    bound_conversations,
                    granted_at: grant.granted_at,
                    last_used_at: grant.last_used_at,
                })
            })
            .collect()
    }

    fn targets(
        &self,
        base: Option<&Path>,
        targets: &[FolderTarget],
    ) -> Result<Vec<GrantTarget>, AppError> {
        targets
            .iter()
            .map(|target| {
                let absolute = resolve(base, &target.path).ok_or_else(|| {
                    if target.path.is_absolute() || base.is_some() {
                        AppError::new(AppErrorCode::ValidationFailed, INVALID_PATH)
                    } else {
                        AppError::new(AppErrorCode::ValidationFailed, NO_WORKING_FOLDER)
                    }
                    .with_detail("path", path_text(&target.path))
                })?;
                let resolved = grant_target(&absolute, target.kind)
                    .map_err(|error| io_error(&absolute, &error))?;
                if self.refuses(&resolved.folder) {
                    return Err(denied(FOLDER_FORBIDDEN, &resolved.folder));
                }
                Ok(resolved)
            })
            .collect()
    }

    pub fn resolve(
        &self,
        call: &FolderCall,
        targets: &[FolderTarget],
    ) -> Result<AccessPlan, AppError> {
        let resolved = match self.owner(&call.conversation_id)? {
            Owner::Conversation(base) => self.targets(base.as_deref(), targets)?,
            Owner::Session(workspace) => {
                let resolved = self.targets(Some(&workspace), targets)?;
                if let Some(outside) = resolved
                    .iter()
                    .find(|target| !covers(&workspace, &target.folder))
                {
                    return Err(denied(OUTSIDE_WORKSPACE, &outside.folder));
                }
                return Ok(AccessPlan {
                    resolved,
                    folders: Vec::new(),
                    used: Vec::new(),
                });
            }
        };
        let grants = self.read(|repositories| {
            Ok(repositories
                .folder_grants
                .applicable(&call.conversation_id)?)
        })?;
        let once = call.once();
        let mut used: Vec<String> = Vec::new();
        let mut uncovered: Vec<PathBuf> = Vec::new();
        for target in &resolved {
            if once.iter().any(|folder| covers(folder, &target.folder)) {
                continue;
            }
            if let Some(grant) = grants
                .iter()
                .find(|grant| covers(Path::new(&grant.path), &target.folder))
            {
                if !used.contains(&grant.id) {
                    used.push(grant.id.clone());
                }
                continue;
            }
            uncovered.push(target.folder.clone());
        }
        Ok(AccessPlan {
            resolved,
            folders: minimal(uncovered),
            used,
        })
    }

    pub async fn ensure_access(
        &self,
        context: &StepContext,
        call: &FolderCall,
        action: &str,
        targets: &[FolderTarget],
    ) -> Result<Vec<GrantTarget>, AppError> {
        context.check_cancelled()?;
        let access = self.plan(call, targets)?;
        access.touch()?;
        if access.needs_grant() {
            crate::access::grant(&self.inner.approvals, context, access.request(action)).await?;
        }
        Ok(access.resolved)
    }

    pub fn approvals(&self) -> &Arc<ApprovalService> {
        &self.inner.approvals
    }

    pub fn plan(
        &self,
        call: &FolderCall,
        targets: &[FolderTarget],
    ) -> Result<FolderAccess, AppError> {
        let plan = self.resolve(call, targets)?;
        Ok(self.access(call, plan))
    }

    pub fn access(&self, call: &FolderCall, plan: AccessPlan) -> FolderAccess {
        let AccessPlan {
            resolved,
            folders,
            used,
        } = plan;
        let entries: Vec<FolderRequestEntryDto> = folders
            .iter()
            .map(|folder| {
                let mut creates: Vec<String> = Vec::new();
                for target in &resolved {
                    let Some(prefix) = relative_to(folder, &target.folder) else {
                        continue;
                    };
                    for created in &target.creates {
                        let name = path_text(&prefix.join(created));
                        if !creates.contains(&name) {
                            creates.push(name);
                        }
                    }
                }
                FolderRequestEntryDto {
                    path: path_text(folder),
                    creates,
                }
            })
            .collect();
        let attendant = Arc::new(FolderAttendant {
            service: self.clone(),
            conversation_id: call.conversation_id.clone(),
            folders: folders.clone(),
            entries: entries.clone(),
            once: Arc::clone(&call.once),
        });
        FolderAccess {
            resolved,
            folders,
            entries,
            used,
            conversation_id: call.conversation_id.clone(),
            tool: call.tool.clone(),
            attendant,
            service: self.clone(),
        }
    }

    pub fn answer(
        &self,
        run_id: &RunId,
        approval_id: &str,
        answer: FolderAccessAnswer,
    ) -> Result<(), AppError> {
        self.answer_as(run_id, approval_id, answer, GRANTED_BY_USER)
    }

    pub fn answer_as(
        &self,
        run_id: &RunId,
        approval_id: &str,
        answer: FolderAccessAnswer,
        granted_by: &str,
    ) -> Result<(), AppError> {
        let run = run_id.to_string();
        let (conversation_id, folders) = {
            let pending = self.pending();
            let entry = pending
                .get(approval_id)
                .filter(|entry| entry.run_id == run && entry.answer.is_none())
                .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_LONGER_PENDING))?;
            (entry.conversation_id.clone(), entry.folders.clone())
        };
        let scope = match answer {
            FolderAccessAnswer::Conversation => Some(FolderGrantScope::Conversation),
            FolderAccessAnswer::Always => Some(FolderGrantScope::Always),
            FolderAccessAnswer::Once | FolderAccessAnswer::Deny => None,
        };
        let mut created: Vec<String> = Vec::new();
        if let Some(scope) = scope {
            for folder in &folders {
                let (grant, fresh) =
                    self.stored_grant(folder, scope, Some(&conversation_id), granted_by)?;
                if fresh {
                    created.push(grant.id);
                }
            }
        }
        if let Some(entry) = self.pending().get_mut(approval_id) {
            entry.answer = Some(answer);
        }
        let decision = if answer == FolderAccessAnswer::Deny {
            ApprovalDecision::Denied
        } else {
            ApprovalDecision::Approved
        };
        if let Err(error) = self
            .inner
            .approvals
            .decide(&run, approval_id, decision, false)
        {
            if let Some(entry) = self.pending().get_mut(approval_id) {
                entry.answer = None;
            }
            for id in &created {
                let _ = self.read(|repositories| Ok(repositories.folder_grants.remove(id)?));
            }
            return Err(error);
        }
        Ok(())
    }
}
