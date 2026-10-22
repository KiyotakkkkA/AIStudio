use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, ConversationId, FILES_APPROVAL_KIND, FOLDER_APPROVAL_KIND, FileDiffDto,
    FolderAccessAnswer, FolderGrantChatDto, FolderGrantDto, FolderGrantId, FolderGrantScope,
    FolderRequestEntryDto, RunId, WorkingFolderAccess,
};

use super::paths::{
    ForbiddenFolders, GrantTarget, TargetKind, canonical, covers, grant_target, relative_to,
    resolve, same_folder,
};
use crate::data::schema::{ConversationRow, FolderGrantRow};
use crate::data::{DataError, Database, Repositories};
use crate::kernel::{ApprovalDecision, DeferredApproval, StepContext};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::services::approvals::{ApprovalService, NO_LONGER_PENDING};
use crate::services::permissions::GLOBAL_SCOPE;

pub const FOLDER_SUBJECT: &str = "folder.access";
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

#[derive(Debug, Clone, Default)]
pub struct Review {
    pub approval: Option<DeferredApproval>,
    pub changes: Vec<FileDiffDto>,
}

pub struct FolderCall {
    conversation_id: String,
    tool: String,
    once: Mutex<Vec<PathBuf>>,
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

    fn hold(&self, folders: &[PathBuf]) {
        self.once
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(folders.iter().cloned());
    }
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
            once: Mutex::new(Vec::new()),
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

    fn covering(
        &self,
        conversation_id: &str,
        folder: &Path,
    ) -> Result<Option<FolderGrantRow>, AppError> {
        Ok(self
            .read(|repositories| Ok(repositories.folder_grants.applicable(conversation_id)?))?
            .into_iter()
            .find(|grant| covers(Path::new(&grant.path), folder)))
    }

    pub fn working_folder_of(&self, conversation_id: &str) -> Result<Option<PathBuf>, AppError> {
        if conversation_id.is_empty() {
            return Ok(None);
        }
        let conversation = self
            .read(|repositories| Ok(repositories.chat.get(conversation_id)?))?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CONVERSATION_MISSING))?;
        Ok(conversation.working_folder.map(PathBuf::from))
    }

    pub fn covered(&self, conversation_id: &str, folder: &Path) -> Result<bool, AppError> {
        self.covering(conversation_id, folder)
            .map(|grant| grant.is_some())
    }

    pub fn grant(
        &self,
        folder: &Path,
        scope: FolderGrantScope,
        conversation_id: Option<&str>,
    ) -> Result<FolderGrantRow, AppError> {
        self.stored_grant(folder, scope, conversation_id)
            .map(|(grant, _)| grant)
    }

    fn stored_grant(
        &self,
        folder: &Path,
        scope: FolderGrantScope,
        conversation_id: Option<&str>,
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
            let created = repositories.folder_grants.create(&FolderGrantRow {
                id: new_id().hyphenated().to_string(),
                path: path_text(folder),
                scope,
                conversation_id: conversation_id.clone(),
                granted_at: now,
                last_used_at: None,
            })?;
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
        conversation_id: &str,
        targets: &[FolderTarget],
    ) -> Result<Vec<GrantTarget>, AppError> {
        let conversation = self
            .read(|repositories| Ok(repositories.chat.get(conversation_id)?))?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CONVERSATION_MISSING))?;
        let base = conversation.working_folder.map(PathBuf::from);
        targets
            .iter()
            .map(|target| {
                let absolute = resolve(base.as_deref(), &target.path).ok_or_else(|| {
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
        let resolved = self.targets(&call.conversation_id, targets)?;
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
        let plan = self.resolve(call, targets)?;
        self.confirm(context, call, action, plan, None).await
    }

    pub async fn confirm(
        &self,
        context: &StepContext,
        call: &FolderCall,
        action: &str,
        plan: AccessPlan,
        review: Option<Review>,
    ) -> Result<Vec<GrantTarget>, AppError> {
        context.check_cancelled()?;
        let AccessPlan {
            resolved,
            folders,
            used,
        } = plan;
        if !used.is_empty() {
            let now = self.inner.clock.now();
            self.read(|repositories| Ok(repositories.folder_grants.touch(&used, now)?))?;
        }
        let approval = review.as_ref().and_then(|review| review.approval.clone());
        if folders.is_empty() {
            let (Some(approval), Some(review)) = (approval, review) else {
                return Ok(resolved);
            };
            let mut request = self.inner.approvals.create(
                &context.run_id.to_string(),
                &approval.subject,
                &approval.scope,
            )?;
            request.insert("kind".to_owned(), json!(FILES_APPROVAL_KIND));
            request.insert("conversationId".to_owned(), json!(call.conversation_id));
            request.insert("tool".to_owned(), json!(call.tool));
            request.insert("action".to_owned(), json!(action));
            request.insert("changes".to_owned(), json!(review.changes));
            return match context.request_approval(request).await? {
                ApprovalDecision::Approved => Ok(resolved),
                ApprovalDecision::Denied => {
                    Err(AppError::new(AppErrorCode::ApprovalDenied, CHANGES_DENIED))
                }
            };
        }
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
        let run_id = context.run_id.to_string();
        let (subject, scope) = approval.map_or_else(
            || (FOLDER_SUBJECT.to_owned(), GLOBAL_SCOPE.to_owned()),
            |approval| (approval.subject, approval.scope),
        );
        let mut request = self.inner.approvals.create(&run_id, &subject, &scope)?;
        request.insert("kind".to_owned(), json!(FOLDER_APPROVAL_KIND));
        request.insert("conversationId".to_owned(), json!(call.conversation_id));
        request.insert("tool".to_owned(), json!(call.tool));
        request.insert("action".to_owned(), json!(action));
        request.insert("folders".to_owned(), json!(entries));
        if let Some(review) = &review
            && !review.changes.is_empty()
        {
            request.insert("changes".to_owned(), json!(review.changes));
        }
        let id = request
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.pending().insert(
            id.clone(),
            Pending {
                run_id,
                conversation_id: call.conversation_id.clone(),
                folders: folders.clone(),
                answer: None,
            },
        );
        let decision = context.request_approval(request).await;
        let answer = self
            .pending()
            .remove(&id)
            .and_then(|pending| pending.answer);
        match (decision?, answer) {
            (ApprovalDecision::Approved, Some(answer)) if answer != FolderAccessAnswer::Deny => {
                call.hold(&folders);
                Ok(resolved)
            }
            _ => Err(
                AppError::new(AppErrorCode::PermissionDenied, FOLDER_ACCESS_DENIED)
                    .with_detail("folders", json!(entries)),
            ),
        }
    }

    pub fn answer(
        &self,
        run_id: &RunId,
        approval_id: &str,
        answer: FolderAccessAnswer,
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
                let (grant, fresh) = self.stored_grant(folder, scope, Some(&conversation_id))?;
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
