use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use async_trait::async_trait;
use regex::{Regex, RegexBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::{check, invalid, js_slice};
use zvs_contract::{
    AppError, AppErrorCode, FILE_CHANGES_MAX, FileChangeKind, FileChangesDto, FileDiffDto,
    ToolTier, Validate,
};

use super::commit::{Fault, commit, snapshot};
use super::ledger::ReadLedger;
use super::plan::{Change, Edit, Plan, RealDisk, plan};
use super::roots::{
    CHAT_ROOT, Located, ROOTS_ONLY, RootKind, SKILL_READ_ONLY, VirtualRoots, is_virtual,
};
use super::text::{Decoded, content_hash, decode};
use super::walk::{WalkOptions, walk};
use crate::chat_files::within_quota;
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, StepContext};
use crate::platform::ids::new_id;
use crate::platform::long_path::extended;
use crate::workspace::{AccessPlan, FolderAccessService, FolderTarget, Review};

pub const FS_LIST: &str = "fs.list";
pub const FS_READ: &str = "fs.read";
pub const FS_SEARCH: &str = "fs.search";
pub const FS_APPLY: &str = "fs.apply";
pub const FILE_TOOLS: [&str; 4] = [FS_LIST, FS_READ, FS_SEARCH, FS_APPLY];

pub const LIST_DESCRIPTION: &str = "Список файлов и папок. Путь — абсолютный, относительный от рабочей папки диалога, chat: (папка файлов диалога) или skill:<ключ>/ (папка навыка, только для чтения). Файлы из .gitignore пропускаются. recursive обходит вложенные папки, glob отбирает файлы по шаблону, например **/*.rs. Доступ к папке вне выданных разрешений подтверждает пользователь.";
pub const READ_DESCRIPTION: &str = "Чтение до 16 текстовых файлов за вызов; для каждого можно указать диапазон строк startLine–endLine. Пути — как в fs.list, например chat:_spec.json или skill:dashboard/references/spec.md. Объём содержимого ограничен, для двоичных файлов сообщается только размер. Существующий файл вне chat: нужно прочитать в текущем ходе, прежде чем изменять его через fs.apply.";
pub const SEARCH_DESCRIPTION: &str = "Поиск текста по файлам папки с учётом .gitignore; путь — как в fs.list. По умолчанию query ищется как обычная строка, regex: true включает регулярные выражения, ignoreCase отключает учёт регистра, glob ограничивает набор файлов. Возвращает путь, номер строки и саму строку для каждого совпадения.";
pub const APPLY_DESCRIPTION: &str = "Набор изменений файлов за один вызов: write записывает файл целиком, edit заменяет фрагменты old на new (каждый old встречается в файле ровно один раз), move перемещает файл, delete удаляет файл. Изменения выполняются по порядку, каждое видит результат предыдущих. Пути — как в fs.list; папки skill: только для чтения. Существующий файл вне chat: перед изменением читается через fs.read в текущем ходе и не должен измениться после чтения. Пользователь подтверждает набор целиком по общему диффу; изменения только в chat: подтверждения не требуют. Набор применяется полностью или не применяется вовсе. Окончания строк и BOM сохраняются, двоичные файлы не изменяются.";

pub const LIST_ACTION: &str = "просмотр папки";
pub const READ_ACTION: &str = "чтение файлов";
pub const SEARCH_ACTION: &str = "поиск по файлам";
pub const APPLY_ACTION: &str = "изменение файлов";

pub const NOT_A_FOLDER: &str = "Путь не является папкой";
pub const FOLDER_MISSING: &str = "Папка не найдена";
pub const INVALID_PATTERN: &str = "Некорректное регулярное выражение";
pub const NOT_READ: &str =
    "Файл не прочитан в этом ходе: прочитайте его через fs.read перед изменением";
pub const STALE: &str = "Файл изменился после чтения: прочитайте его заново через fs.read";
pub const COMMIT_FAILED: &str =
    "Изменения не применены: не удалось записать файл, уже записанные файлы восстановлены";
pub const RESTORE_FAILED: &str = "Изменения не применены: не удалось записать файл, и часть файлов не удалось восстановить; исходное содержимое сохранено в снимке";
pub const SNAPSHOT_FAILED: &str =
    "Изменения не применены: не удалось сохранить снимок исходных файлов";
pub const WORKER_FAILED: &str = "Фоновая операция с файлами прервалась";

pub const LIST_LIMIT_DEFAULT: u64 = 200;
pub const LIST_LIMIT_MAX: u64 = 2000;
pub const READ_PATHS_MAX: usize = 16;
pub const READ_FILE_CHARS: usize = 20_000;
pub const READ_TOTAL_CHARS: usize = 60_000;
pub const READ_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const SEARCH_LIMIT_DEFAULT: u64 = 100;
pub const SEARCH_LIMIT_MAX: u64 = 500;
pub const SEARCH_FILES_MAX: usize = 20_000;
pub const SEARCH_FILE_MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const SEARCH_LINE_CHARS: usize = 300;
pub const QUERY_MAX_CHARS: usize = 1000;
pub const PATH_MAX_CHARS: usize = 32_767;
pub const APPLY_CONTENT_MAX: usize = 1024 * 1024;
pub const APPLY_EDITS_MAX: usize = 64;

static FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(/?)(file|listing|matches)").expect("a valid pattern"));

pub fn fenced(text: &str) -> String {
    FENCE.replace_all(text, "<$1 $2").into_owned()
}

fn worker_failed() -> AppError {
    AppError::new(AppErrorCode::NativeError, WORKER_FAILED)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn check_path(field: &str, value: &str) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, PATH_MAX_CHARS)
}

fn check_optional_path(field: &str, value: Option<&str>) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check_path(field, value))
}

fn limit(value: Option<u64>, default: u64, max: u64) -> usize {
    usize::try_from(value.unwrap_or(default).clamp(1, max)).unwrap_or(usize::MAX)
}

fn outside_roots() -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, ROOTS_ONLY)
}

fn read_only_skill() -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, SKILL_READ_ONLY)
}

fn listed_path(path: Option<&str>, roots_only: bool) -> Option<&str> {
    path.or_else(|| roots_only.then_some(CHAT_ROOT))
}

fn listing_permission(tool: &str, input: &Value) -> PermissionRequirement {
    let only = input.get("rootsOnly").and_then(Value::as_bool) == Some(true);
    match listed_path(input.get("path").and_then(Value::as_str), only) {
        Some(path) if is_virtual(path) => PermissionRequirement::app_folder(tool),
        _ => PermissionRequirement::tool(tool),
    }
}

fn all_virtual<'a>(paths: impl IntoIterator<Item = Option<&'a str>>) -> bool {
    let mut any = false;
    let all = paths.into_iter().all(|path| {
        any = true;
        path.is_some_and(is_virtual)
    });
    any && all
}

#[derive(Clone)]
pub struct FileToolOptions {
    pub runs_dir: PathBuf,
    pub ledger: ReadLedger,
    pub fault: Option<Fault>,
    pub roots: VirtualRoots,
}

impl FileToolOptions {
    pub fn new(runs_dir: PathBuf) -> Self {
        Self {
            runs_dir,
            ledger: ReadLedger::new(),
            fault: None,
            roots: VirtualRoots::default(),
        }
    }
}

#[derive(Clone)]
struct Tools {
    folders: FolderAccessService,
    options: FileToolOptions,
}

impl Tools {
    fn located(&self, conversation: &str, path: &str) -> Result<Option<Located>, AppError> {
        self.options.roots.locate(conversation, path)
    }

    async fn folder(
        &self,
        context: &StepContext,
        conversation: &str,
        tool: &str,
        action: &str,
        path: Option<&str>,
        roots_only: bool,
    ) -> Result<PathBuf, AppError> {
        let path = listed_path(path, roots_only);
        if let Some(located) = path
            .map(|path| self.located(conversation, path))
            .transpose()?
            .flatten()
        {
            return match std::fs::metadata(extended(&located.path)) {
                Ok(metadata) if metadata.is_dir() => Ok(located.path),
                Ok(_) => Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_FOLDER)
                    .with_detail("path", path_text(&located.path))),
                Err(_) => Err(AppError::new(AppErrorCode::NotFound, FOLDER_MISSING)
                    .with_detail("path", path_text(&located.path))),
            };
        }
        if roots_only {
            return Err(outside_roots());
        }
        let call = self.folders.call(conversation, tool);
        let target = FolderTarget::folder(path.unwrap_or("."));
        let resolved = self
            .folders
            .ensure_access(context, &call, action, std::slice::from_ref(&target))
            .await?;
        let root = resolved
            .into_iter()
            .next()
            .map(|target| target.path)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, FOLDER_MISSING))?;
        match std::fs::metadata(extended(&root)) {
            Ok(metadata) if metadata.is_dir() => Ok(root),
            Ok(_) => Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_FOLDER)
                .with_detail("path", path_text(&root))),
            Err(_) => Err(AppError::new(AppErrorCode::NotFound, FOLDER_MISSING)
                .with_detail("path", path_text(&root))),
        }
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| worker_failed())?
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsListInput {
    #[serde(default)]
    #[schemars(
        description = "Папка: абсолютный путь, путь относительно рабочей папки, chat: или skill:<ключ>/; по умолчанию рабочая папка"
    )]
    pub path: Option<String>,
    #[serde(default)]
    #[schemars(description = "Обходить вложенные папки")]
    pub recursive: bool,
    #[serde(default)]
    #[schemars(description = "Шаблон для отбора файлов, например **/*.ts")]
    pub glob: Option<String>,
    #[serde(default)]
    #[schemars(description = "Наибольшее число записей, по умолчанию 200, не более 2000")]
    pub limit: Option<u64>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
    #[serde(default)]
    #[schemars(skip)]
    pub roots_only: bool,
}

impl Validate for FsListInput {
    fn validate(&self) -> Result<(), AppError> {
        check_optional_path("path", self.path.as_deref())?;
        if let Some(glob) = &self.glob {
            check::max_len("glob", glob, QUERY_MAX_CHARS)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsListOutput {
    pub text: String,
    pub entries: u64,
    pub truncated: bool,
}

impl Validate for FsListOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("entries", self.entries)
    }
}

struct FsListNode(Tools);

#[async_trait]
impl NodeDef for FsListNode {
    type Input = FsListInput;
    type Output = FsListOutput;

    fn node_type(&self) -> &str {
        FS_LIST
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(FS_LIST)
    }

    fn permission_for(&self, input: &Value) -> PermissionRequirement {
        listing_permission(FS_LIST, input)
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: FsListInput,
    ) -> Result<FsListOutput, AppError> {
        let root = self
            .0
            .folder(
                context,
                &input.conversation_id,
                FS_LIST,
                LIST_ACTION,
                input.path.as_deref(),
                input.roots_only,
            )
            .await?;
        let cancel = context.cancel.clone();
        let count = limit(input.limit, LIST_LIMIT_DEFAULT, LIST_LIMIT_MAX);
        blocking(move || {
            let listing = walk(
                &root,
                WalkOptions {
                    recursive: input.recursive,
                    folders: !input.recursive || input.glob.is_none(),
                    glob: input.glob.as_deref(),
                    limit: count,
                },
                &cancel,
            )?;
            let lines: Vec<String> = listing
                .entries
                .iter()
                .map(|entry| {
                    if entry.folder {
                        format!("{}/", entry.relative)
                    } else if entry.link {
                        format!("{} (link)", entry.relative)
                    } else {
                        format!("{} ({} bytes)", entry.relative, entry.bytes)
                    }
                })
                .collect();
            let body = if lines.is_empty() {
                "(empty)".to_owned()
            } else {
                fenced(&lines.join("\n"))
            };
            let mut text = format!("Folder: {}\n<listing>\n{body}\n</listing>", root.display());
            if listing.truncated {
                text.push_str(&format!("\n[listing stopped at {count} entries]"));
            }
            Ok(FsListOutput {
                text,
                entries: listing.entries.len() as u64,
                truncated: listing.truncated,
            })
        })
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsReadTarget {
    #[schemars(
        description = "Файл: абсолютный путь, путь относительно рабочей папки, chat:<путь> или skill:<ключ>/<путь>"
    )]
    pub path: String,
    #[serde(default)]
    #[schemars(description = "Первая строка, начиная с 1")]
    pub start_line: Option<u64>,
    #[serde(default)]
    #[schemars(description = "Последняя строка включительно")]
    pub end_line: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsReadInput {
    #[schemars(description = "Файлы для чтения, не более 16")]
    pub paths: Vec<FsReadTarget>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
    #[serde(default)]
    #[schemars(skip)]
    pub roots_only: bool,
}

impl Validate for FsReadInput {
    fn validate(&self) -> Result<(), AppError> {
        if self.paths.is_empty() {
            return Err(invalid("paths", "minItems"));
        }
        if self.paths.len() > READ_PATHS_MAX {
            return Err(invalid("paths", "maxItems"));
        }
        for (index, target) in self.paths.iter().enumerate() {
            check_path(&format!("paths.{index}.path"), &target.path)?;
            if target.start_line == Some(0) {
                return Err(invalid(&format!("paths.{index}.startLine"), "minimum"));
            }
            if let (Some(start), Some(end)) = (target.start_line, target.end_line)
                && end < start
            {
                return Err(invalid(&format!("paths.{index}.endLine"), "minimum"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsReadFile {
    pub path: String,
    pub status: String,
    pub lines: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsReadOutput {
    pub text: String,
    pub files: Vec<FsReadFile>,
}

impl Validate for FsReadOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

struct ReadBudget {
    remaining: usize,
}

fn read_one(
    path: &Path,
    request: &FsReadTarget,
    budget: &mut ReadBudget,
    ledger: &ReadLedger,
    run: &str,
) -> (String, FsReadFile) {
    let shown = path.display();
    let entry = |status: &str, lines: u64, truncated: bool| FsReadFile {
        path: path_text(path),
        status: status.to_owned(),
        lines,
        truncated,
    };
    let metadata = match std::fs::metadata(extended(path)) {
        Ok(metadata) => metadata,
        Err(_) => {
            return (
                format!("<file path=\"{shown}\">\n[not found]\n</file>"),
                entry("missing", 0, false),
            );
        }
    };
    if metadata.is_dir() {
        return (
            format!("<file path=\"{shown}\">\n[a folder; list it with fs.list]\n</file>"),
            entry("folder", 0, false),
        );
    }
    if metadata.len() > READ_MAX_BYTES {
        return (
            format!(
                "<file path=\"{shown}\">\n[too large to read: {} bytes]\n</file>",
                metadata.len()
            ),
            entry("tooLarge", 0, false),
        );
    }
    let Ok(bytes) = std::fs::read(extended(path)) else {
        return (
            format!("<file path=\"{shown}\">\n[could not be read]\n</file>"),
            entry("unreadable", 0, false),
        );
    };
    let text = match decode(&bytes) {
        Decoded::Binary => {
            return (
                format!(
                    "<file path=\"{shown}\">\n[binary or non-UTF-8 file, {} bytes; its contents are not returned]\n</file>",
                    bytes.len()
                ),
                entry("binary", 0, false),
            );
        }
        Decoded::Text { text, .. } => text,
    };
    ledger.record(run, path, &content_hash(&bytes));
    let lines: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        text.strip_suffix('\n')
            .unwrap_or(&text)
            .split('\n')
            .collect()
    };
    let total = lines.len();
    let start = usize::try_from(request.start_line.unwrap_or(1)).unwrap_or(usize::MAX);
    let end = request
        .end_line
        .and_then(|end| usize::try_from(end).ok())
        .unwrap_or(total)
        .min(total);
    if total == 0 || start > total {
        return (
            format!(
                "<file path=\"{shown}\" total=\"{total}\">\n[no lines in this range; the file has {total} lines]\n</file>"
            ),
            entry("read", total as u64, false),
        );
    }
    let allowance = READ_FILE_CHARS.min(budget.remaining);
    if allowance == 0 {
        return (
            format!(
                "<file path=\"{shown}\" total=\"{total}\">\n[not shown: the read budget of this call is used up; read it in another call]\n</file>"
            ),
            entry("read", total as u64, true),
        );
    }
    let mut body = String::new();
    let mut last = start - 1;
    for line in &lines[start - 1..end] {
        if body.len() + line.len() + 1 > allowance && last >= start {
            break;
        }
        let room = allowance.saturating_sub(body.len());
        body.push_str(js_slice(line, room));
        body.push('\n');
        last += 1;
    }
    budget.remaining = budget.remaining.saturating_sub(body.len());
    let truncated = last < end;
    let mut section = format!(
        "<file path=\"{shown}\" lines=\"{start}-{last}\" total=\"{total}\">\n{}</file>",
        fenced(&body)
    );
    if truncated {
        section.push_str(&format!(
            "\n[truncated after line {last}; continue with startLine {}]",
            last + 1
        ));
    }
    (section, entry("read", total as u64, truncated))
}

struct FsReadNode(Tools);

#[async_trait]
impl NodeDef for FsReadNode {
    type Input = FsReadInput;
    type Output = FsReadOutput;

    fn node_type(&self) -> &str {
        FS_READ
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(FS_READ)
    }

    fn permission_for(&self, input: &Value) -> PermissionRequirement {
        let paths = input
            .get("paths")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if all_virtual(
            paths
                .iter()
                .map(|target| target.get("path").and_then(Value::as_str)),
        ) {
            PermissionRequirement::app_folder(FS_READ)
        } else {
            self.permission()
        }
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: FsReadInput,
    ) -> Result<FsReadOutput, AppError> {
        let mut located: Vec<Option<PathBuf>> = Vec::with_capacity(input.paths.len());
        for target in &input.paths {
            located.push(
                self.0
                    .located(&input.conversation_id, &target.path)?
                    .map(|found| found.path),
            );
        }
        let targets: Vec<FolderTarget> = input
            .paths
            .iter()
            .zip(&located)
            .filter(|(_, found)| found.is_none())
            .map(|(target, _)| FolderTarget::file(&target.path))
            .collect();
        if input.roots_only && !targets.is_empty() {
            return Err(outside_roots());
        }
        let granted = if targets.is_empty() {
            Vec::new()
        } else {
            let call = self.0.folders.call(&input.conversation_id, FS_READ);
            self.0
                .folders
                .ensure_access(context, &call, READ_ACTION, &targets)
                .await?
        };
        let mut granted = granted.into_iter().map(|target| target.path);
        let resolved: Vec<PathBuf> = located
            .into_iter()
            .map(|found| found.or_else(|| granted.next()).unwrap_or_default())
            .collect();
        let ledger = self.0.options.ledger.clone();
        let run = context.run_id.to_string();
        blocking(move || {
            let mut budget = ReadBudget {
                remaining: READ_TOTAL_CHARS,
            };
            let mut sections = Vec::with_capacity(resolved.len());
            let mut files = Vec::with_capacity(resolved.len());
            for (path, request) in resolved.iter().zip(&input.paths) {
                let (section, file) = read_one(path, request, &mut budget, &ledger, &run);
                sections.push(section);
                files.push(file);
            }
            Ok(FsReadOutput {
                text: sections.join("\n"),
                files,
            })
        })
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsSearchInput {
    #[schemars(description = "Искомая строка или регулярное выражение")]
    pub query: String,
    #[serde(default)]
    #[schemars(description = "Папка поиска; по умолчанию рабочая папка")]
    pub path: Option<String>,
    #[serde(default)]
    #[schemars(description = "Считать query регулярным выражением")]
    pub regex: bool,
    #[serde(default)]
    #[schemars(description = "Не учитывать регистр")]
    pub ignore_case: bool,
    #[serde(default)]
    #[schemars(description = "Шаблон для отбора файлов, например **/*.md")]
    pub glob: Option<String>,
    #[serde(default)]
    #[schemars(description = "Наибольшее число совпадений, по умолчанию 100, не более 500")]
    pub limit: Option<u64>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
    #[serde(default)]
    #[schemars(skip)]
    pub roots_only: bool,
}

impl Validate for FsSearchInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("query", &self.query)?;
        check::max_len("query", &self.query, QUERY_MAX_CHARS)?;
        check_optional_path("path", self.path.as_deref())?;
        if let Some(glob) = &self.glob {
            check::max_len("glob", glob, QUERY_MAX_CHARS)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsSearchOutput {
    pub text: String,
    pub matches: u64,
    pub truncated: bool,
}

impl Validate for FsSearchOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("matches", self.matches)
    }
}

fn search(
    root: &Path,
    matcher: &Regex,
    glob: Option<&str>,
    count: usize,
    cancel: &CancellationToken,
) -> Result<FsSearchOutput, AppError> {
    let listing = walk(
        root,
        WalkOptions {
            recursive: true,
            folders: false,
            glob,
            limit: SEARCH_FILES_MAX,
        },
        cancel,
    )?;
    let mut found: Vec<String> = Vec::new();
    let mut truncated = listing.truncated;
    'files: for entry in &listing.entries {
        if cancel.is_cancelled() {
            return Err(crate::kernel::types::cancelled());
        }
        if entry.link || entry.bytes > SEARCH_FILE_MAX_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(extended(&entry.path)) else {
            continue;
        };
        let Decoded::Text { text, .. } = decode(&bytes) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if !matcher.is_match(line) {
                continue;
            }
            if found.len() >= count {
                truncated = true;
                break 'files;
            }
            found.push(format!(
                "{}:{}: {}",
                entry.relative,
                number + 1,
                js_slice(line.trim_end(), SEARCH_LINE_CHARS)
            ));
        }
    }
    let body = if found.is_empty() {
        "(no matches)".to_owned()
    } else {
        fenced(&found.join("\n"))
    };
    let mut text = format!(
        "Folder: {}\nMatches: {}\n<matches>\n{body}\n</matches>",
        root.display(),
        found.len()
    );
    if truncated {
        text.push_str("\n[search stopped early; narrow the query, path or glob]");
    }
    Ok(FsSearchOutput {
        text,
        matches: found.len() as u64,
        truncated,
    })
}

struct FsSearchNode(Tools);

#[async_trait]
impl NodeDef for FsSearchNode {
    type Input = FsSearchInput;
    type Output = FsSearchOutput;

    fn node_type(&self) -> &str {
        FS_SEARCH
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(FS_SEARCH)
    }

    fn permission_for(&self, input: &Value) -> PermissionRequirement {
        listing_permission(FS_SEARCH, input)
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: FsSearchInput,
    ) -> Result<FsSearchOutput, AppError> {
        let pattern = if input.regex {
            input.query.clone()
        } else {
            regex::escape(&input.query)
        };
        let matcher = RegexBuilder::new(&pattern)
            .case_insensitive(input.ignore_case)
            .size_limit(1 << 20)
            .build()
            .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, INVALID_PATTERN))?;
        let root = self
            .0
            .folder(
                context,
                &input.conversation_id,
                FS_SEARCH,
                SEARCH_ACTION,
                input.path.as_deref(),
                input.roots_only,
            )
            .await?;
        let cancel = context.cancel.clone();
        let count = limit(input.limit, SEARCH_LIMIT_DEFAULT, SEARCH_LIMIT_MAX);
        blocking(move || search(&root, &matcher, input.glob.as_deref(), count, &cancel)).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsEdit {
    #[schemars(
        description = "Точный фрагмент текущего содержимого файла; должен встречаться ровно один раз"
    )]
    pub old: String,
    #[schemars(description = "Текст, которым заменяется фрагмент")]
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum FsChange {
    Write { path: String, content: String },
    Edit { path: String, edits: Vec<FsEdit> },
    Move { from: String, to: String },
    Delete { path: String },
}

impl FsChange {
    fn weight(&self) -> usize {
        match self {
            Self::Write { content, .. } => content.len(),
            Self::Edit { edits, .. } => edits
                .iter()
                .map(|edit| edit.old.len() + edit.new.len())
                .sum(),
            Self::Move { .. } | Self::Delete { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsApplyInput {
    #[schemars(description = "Изменения по порядку, от 1 до 32, не более 1 МиБ текста в сумме")]
    pub changes: Vec<FsChange>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
    #[serde(default)]
    #[schemars(skip)]
    pub roots_only: bool,
}

impl Validate for FsApplyInput {
    fn validate(&self) -> Result<(), AppError> {
        if self.changes.is_empty() {
            return Err(invalid("changes", "minItems"));
        }
        if self.changes.len() > FILE_CHANGES_MAX {
            return Err(invalid("changes", "maxItems"));
        }
        if self.changes.iter().map(FsChange::weight).sum::<usize>() > APPLY_CONTENT_MAX {
            return Err(invalid("changes", "maxBytes"));
        }
        for (index, change) in self.changes.iter().enumerate() {
            let field = |name: &str| format!("changes.{index}.{name}");
            match change {
                FsChange::Write { path, .. } | FsChange::Delete { path } => {
                    check_path(&field("path"), path)?;
                }
                FsChange::Edit { path, edits } => {
                    check_path(&field("path"), path)?;
                    if edits.is_empty() {
                        return Err(invalid(&field("edits"), "minItems"));
                    }
                    if edits.len() > APPLY_EDITS_MAX {
                        return Err(invalid(&field("edits"), "maxItems"));
                    }
                }
                FsChange::Move { from, to } => {
                    check_path(&field("from"), from)?;
                    check_path(&field("to"), to)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FsApplyOutput {
    pub text: String,
    pub changes: Vec<FileDiffDto>,
    pub paths: Vec<String>,
}

impl Validate for FsApplyOutput {
    fn validate(&self) -> Result<(), AppError> {
        FileChangesDto {
            changes: self.changes.clone(),
        }
        .validate()
    }
}

pub fn deletes(input: &Value) -> bool {
    input
        .get("changes")
        .and_then(Value::as_array)
        .is_some_and(|changes| {
            changes
                .iter()
                .any(|change| change.get("op").and_then(Value::as_str) == Some("delete"))
        })
}

fn change_paths(changes: &[FsChange]) -> Vec<&str> {
    changes
        .iter()
        .flat_map(|change| match change {
            FsChange::Write { path, .. }
            | FsChange::Edit { path, .. }
            | FsChange::Delete { path } => {
                vec![path.as_str()]
            }
            FsChange::Move { from, to } => vec![from.as_str(), to.as_str()],
        })
        .collect()
}

fn field<'a>(change: &'a Value, name: &str) -> Option<&'a str> {
    change.get(name).and_then(Value::as_str)
}

fn input_paths(input: &Value) -> Vec<Option<&str>> {
    input
        .get("changes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .flat_map(|change| match change.get("op").and_then(Value::as_str) {
            Some("move") => vec![field(change, "from"), field(change, "to")],
            _ => vec![field(change, "path")],
        })
        .collect()
}

fn resolved_changes(changes: &[FsChange], resolved: &[PathBuf]) -> Vec<Change> {
    let mut paths = resolved.iter().cloned();
    let mut next = || paths.next().unwrap_or_default();
    changes
        .iter()
        .map(|change| match change {
            FsChange::Write { content, .. } => Change::Write {
                path: next(),
                content: content.clone(),
            },
            FsChange::Edit { edits, .. } => Change::Edit {
                path: next(),
                edits: edits
                    .iter()
                    .map(|edit| Edit {
                        old: edit.old.clone(),
                        new: edit.new.clone(),
                    })
                    .collect(),
            },
            FsChange::Move { .. } => Change::Move {
                from: next(),
                to: next(),
            },
            FsChange::Delete { .. } => Change::Delete { path: next() },
        })
        .collect()
}

fn check_fresh(
    planned: &Plan,
    ledger: &ReadLedger,
    run: &str,
    exempt: &[PathBuf],
) -> Result<(), AppError> {
    for path in &planned.fresh {
        if exempt.iter().any(|base| path.starts_with(base)) {
            continue;
        }
        let Some(before) = planned
            .files
            .iter()
            .find(|file| file.path == *path)
            .and_then(|file| file.before.as_ref())
        else {
            continue;
        };
        let refused = |message: &str| {
            AppError::new(AppErrorCode::Conflict, message).with_detail("path", path_text(path))
        };
        match ledger.hash(run, path) {
            None => return Err(refused(NOT_READ)),
            Some(hash) if hash != content_hash(before) => return Err(refused(STALE)),
            Some(_) => {}
        }
    }
    Ok(())
}

fn unchanged_since_plan(planned: &Plan) -> Result<(), AppError> {
    for file in &planned.files {
        let current = match std::fs::read(extended(&file.path)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => Some(Vec::new()),
        };
        if current != file.before {
            return Err(AppError::new(AppErrorCode::Conflict, STALE)
                .with_detail("path", path_text(&file.path)));
        }
    }
    Ok(())
}

fn summary(changes: &[FileDiffDto]) -> String {
    if changes.is_empty() {
        return "No file changed: the batch leaves every file as it was.".to_owned();
    }
    let mut lines = vec![format!("Applied {} file changes:", changes.len())];
    for change in changes {
        lines.push(match change.kind {
            FileChangeKind::Create => format!("created {} (+{})", change.path, change.added),
            FileChangeKind::Modify => format!(
                "modified {} (+{} -{})",
                change.path, change.added, change.removed
            ),
            FileChangeKind::Move => format!(
                "moved {} -> {} (+{} -{})",
                change.from.as_deref().unwrap_or_default(),
                change.path,
                change.added,
                change.removed
            ),
            FileChangeKind::Delete => format!("deleted {} (-{})", change.path, change.removed),
        });
    }
    lines.join("\n")
}

struct FsApplyNode(Tools);

#[async_trait]
impl NodeDef for FsApplyNode {
    type Input = FsApplyInput;
    type Output = FsApplyOutput;

    fn node_type(&self) -> &str {
        FS_APPLY
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(FS_APPLY)
    }

    fn permission_for(&self, input: &Value) -> PermissionRequirement {
        if all_virtual(input_paths(input)) {
            PermissionRequirement::app_folder(FS_APPLY)
        } else if deletes(input) {
            PermissionRequirement::tool_at(FS_APPLY, ToolTier::Ask)
        } else {
            self.permission()
        }
    }

    fn defers_approval(&self) -> bool {
        true
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: FsApplyInput,
    ) -> Result<FsApplyOutput, AppError> {
        let approval = context.take_deferred_approval();
        let tools = &self.0;
        let paths = change_paths(&input.changes);
        let mut located: Vec<Option<Located>> = Vec::with_capacity(paths.len());
        for path in &paths {
            let found = tools.located(&input.conversation_id, path)?;
            if found
                .as_ref()
                .is_some_and(|found| found.kind == RootKind::Skill)
            {
                return Err(read_only_skill());
            }
            located.push(found);
        }
        let real: Vec<FolderTarget> = paths
            .iter()
            .zip(&located)
            .filter(|(_, found)| found.is_none())
            .map(|(path, _)| FolderTarget::file(path))
            .collect();
        if input.roots_only && !real.is_empty() {
            return Err(outside_roots());
        }
        let call = tools.folders.call(&input.conversation_id, FS_APPLY);
        let access = if real.is_empty() {
            AccessPlan::empty()
        } else {
            tools.folders.resolve(&call, &real)?
        };
        let mut bases: Vec<PathBuf> = Vec::new();
        for found in located.iter().flatten() {
            if !bases.contains(&found.base) {
                bases.push(found.base.clone());
            }
        }
        let mut granted = access.resolved.iter().map(|target| target.path.clone());
        let resolved: Vec<PathBuf> = located
            .into_iter()
            .map(|found| {
                found
                    .map(|found| found.path)
                    .or_else(|| granted.next())
                    .unwrap_or_default()
            })
            .collect();
        let changes = resolved_changes(&input.changes, &resolved);
        let planned = blocking(move || plan(&changes, &RealDisk)).await?;
        let run = context.run_id.to_string();
        check_fresh(&planned, &tools.options.ledger, &run, &bases)?;
        for base in &bases {
            let sized: Vec<(PathBuf, Option<u64>)> = planned
                .files
                .iter()
                .filter(|file| file.path.starts_with(base))
                .map(|file| {
                    (
                        file.path.clone(),
                        file.after.as_ref().map(|bytes| bytes.len() as u64),
                    )
                })
                .collect();
            within_quota(base, &sized)?;
        }
        if planned.files.is_empty() {
            return Ok(FsApplyOutput {
                text: summary(&[]),
                changes: Vec::new(),
                paths: Vec::new(),
            });
        }
        tools
            .folders
            .confirm(
                context,
                &call,
                APPLY_ACTION,
                access,
                Some(Review {
                    approval,
                    changes: planned.changes.clone(),
                }),
            )
            .await?;
        context.check_cancelled()?;
        let folder = tools
            .options
            .runs_dir
            .join(&run)
            .join("snapshots")
            .join(new_id().simple().to_string());
        let fault = tools.options.fault.clone();
        let committed = planned.clone();
        blocking(move || {
            unchanged_since_plan(&committed)?;
            snapshot(&folder, &committed.files)
                .map_err(|_| AppError::new(AppErrorCode::NativeError, SNAPSHOT_FAILED))?;
            commit(&committed.files, fault.as_ref()).map_err(|failure| {
                let message = if failure.restored {
                    COMMIT_FAILED
                } else {
                    RESTORE_FAILED
                };
                AppError::new(AppErrorCode::NativeError, message)
                    .with_detail("path", path_text(&failure.path))
                    .with_detail("snapshot", path_text(&folder))
            })
        })
        .await?;
        for file in &planned.files {
            match &file.after {
                Some(bytes) => tools
                    .options
                    .ledger
                    .record(&run, &file.path, &content_hash(bytes)),
                None => tools.options.ledger.forget(&run, &file.path),
            }
        }
        Ok(FsApplyOutput {
            text: summary(&planned.changes),
            paths: planned
                .files
                .iter()
                .map(|file| path_text(&file.path))
                .collect(),
            changes: planned.changes,
        })
    }

    fn tool_meta(&self, _input: &Value, output: &FsApplyOutput) -> Option<Value> {
        Some(json!(FileChangesDto {
            changes: output.changes.clone(),
        }))
    }
}

pub fn register_file_nodes(
    nodes: &NodeRegistry,
    folders: &FolderAccessService,
    options: FileToolOptions,
) -> Result<(), AppError> {
    let tools = Tools {
        folders: folders.clone(),
        options,
    };
    nodes.register(FsListNode(tools.clone()))?;
    nodes.register(FsReadNode(tools.clone()))?;
    nodes.register(FsSearchNode(tools.clone()))?;
    nodes.register(FsApplyNode(tools))
}
