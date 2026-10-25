use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AppError, AppErrorCode, CreateSkillInput, SkillDetailDto, SkillDto, SkillFileDto,
    SkillFileEncoding, SkillFileInput, SkillId, SkillIssueDto, SkillIssueRule, SkillSource,
    SkillStatus, SkillUpdateDto, UpdateSkillInput, is_skill_key, is_skill_path,
    skills::{
        SKILL_AUTHOR_MAX, SKILL_FILE_MAX_BYTES, SKILL_FILES_MAX, SKILL_KEY_MAX, SKILL_NAME_MAX,
        SKILL_TOTAL_MAX_BYTES, SKILL_VERSION_MAX,
    },
};

use super::bundled::{BUNDLED_SKILLS, BundledSkill, is_newer};
use super::manifest::{
    AUTHOR, Manifest, READ_FIRST, REQUIRES, Requires, SKILL_FILE, VERSION, meta, parse_manifest,
    read_first, render_manifest, requires,
};
use super::purpose::purpose_of;
use crate::data::schema::SkillRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub const SKILLS_DIR: &str = "skills";
pub const SKILL_NOT_FOUND: &str = "Навык не найден";
pub const SKILL_INVALID: &str = "Навык не прошёл проверку";
pub const SKILL_NOT_READY: &str = "Навык содержит ошибки и не может быть использован";
pub const SKILL_DISABLED: &str = "Навык выключен";
pub const SKILL_FILE_NOT_FOUND: &str = "Файл навыка не найден";
pub const SKILL_SCRIPT_OUTSIDE: &str = "Запускать можно только файлы из папки scripts/";
pub const SKILL_FILE_TOO_LARGE: &str = "Файл навыка превышает допустимый размер";
pub const SKILL_STORAGE_FAILED: &str = "Не удалось сохранить файлы навыка";
pub const SKILL_IMPORT_NO_MANIFEST: &str = "В выбранной папке нет файла SKILL.md";
pub const SKILL_EXPORT_TAKEN: &str =
    "В папке назначения уже есть папка с идентификатором этого навыка";
pub const SKILL_UP_TO_DATE: &str = "Для навыка нет обновления";
pub const SKILL_BUNDLED_PROTECTED: &str =
    "Встроенный навык нельзя перезаписать. Сохраните навык под другим идентификатором";
pub const AUTHORED_VERSION: &str = "1.0.0";

const BUNDLED_MARKER: &str = ".bundled.json";
const COPY_SUFFIX: &str = "-copy";
const COPY_NAME_SUFFIX: &str = " (копия)";
const STAGING_PREFIX: &str = ".staging-";
const SCRIPTS_PREFIX: &str = "scripts/";
const TRASH_PREFIX: &str = ".trash-";

pub struct SkillLibraryOptions {
    pub database: Arc<Database>,
    pub root: PathBuf,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub bundled: &'static [BundledSkill],
}

impl SkillLibraryOptions {
    pub fn new(database: Arc<Database>, root: PathBuf) -> Self {
        Self {
            database,
            root,
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
            bundled: BUNDLED_SKILLS,
        }
    }
}

struct Inner {
    database: Arc<Database>,
    root: PathBuf,
    logger: SharedLogger,
    clock: SharedClock,
    bundled: &'static [BundledSkill],
    writing: Mutex<()>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Seeded {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Marker {
    Installed(BTreeMap<String, Seeded>),
    Keys(BTreeSet<String>),
}

#[derive(Clone)]
pub struct SkillLibrary {
    inner: Arc<Inner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSkill {
    pub skill: SkillDto,
    pub instructions: String,
    pub files: Vec<LoadedFile>,
    pub read_first: Vec<String>,
    pub requires: Requires,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    pub path: String,
    pub size: u64,
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillScriptFile {
    pub path: String,
    pub dir: PathBuf,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthoredSkill {
    pub key: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub files: Vec<(String, String)>,
    pub version: Option<String>,
    pub read_first: Vec<String>,
    pub requires: Requires,
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFileContent {
    pub path: String,
    pub size: u64,
    pub text: Option<String>,
}

enum Source {
    Disk(PathBuf),
    Bytes(Vec<u8>),
}

struct Staged {
    field: String,
    size: u64,
    source: Source,
}

struct Draft {
    key: String,
    name: String,
    description: String,
    instructions: String,
    enabled: bool,
    source: SkillSource,
    extra: Vec<(String, String)>,
    metadata: Vec<(String, String)>,
    files: BTreeMap<String, Staged>,
    issues: Vec<SkillIssueDto>,
}

struct Scanned {
    manifest: Option<String>,
    files: Vec<(String, PathBuf, u64)>,
    strays: Vec<String>,
    fingerprint: String,
}

fn issue(field: impl Into<String>, rule: SkillIssueRule) -> SkillIssueDto {
    SkillIssueDto {
        field: field.into(),
        rule,
    }
}

fn rejected(issues: &[SkillIssueDto]) -> AppError {
    let mut error = AppError::new(AppErrorCode::ValidationFailed, SKILL_INVALID);
    if let Some(first) = issues.first() {
        error = error
            .with_detail("field", first.field.clone())
            .with_detail("rule", first.rule.as_str());
    }
    error.with_detail("issues", json!(issues))
}

fn storage(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, SKILL_STORAGE_FAILED)
        .with_detail("reason", error.to_string())
}

fn not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, SKILL_NOT_FOUND)
}

fn malformed() -> AppError {
    AppError::new(AppErrorCode::Unknown, "Навык хранится в повреждённом виде")
}

fn walk(
    dir: &Path,
    base: &Path,
    out: &mut Vec<(String, PathBuf, u64, u128)>,
) -> std::io::Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk(&path, base, out)?;
            continue;
        }
        let metadata = entry.metadata()?;
        let relative = path
            .strip_prefix(base)
            .map_err(|_| std::io::Error::other("outside the skill folder"))?
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_nanos());
        out.push((relative, path, metadata.len(), modified));
    }
    Ok(())
}

fn scan(dir: &Path) -> std::io::Result<Scanned> {
    let mut entries = Vec::new();
    walk(dir, dir, &mut entries)?;
    let mut hasher = Sha256::new();
    let mut manifest = None;
    let mut files = Vec::new();
    let mut strays = Vec::new();
    for (relative, path, size, modified) in entries {
        hasher.update(format!("{relative}\u{0}{size}\u{0}{modified}\n").as_bytes());
        if relative == SKILL_FILE {
            manifest = Some(fs::read_to_string(&path)?);
        } else if is_skill_path(&relative) {
            files.push((relative, path, size));
        } else {
            strays.push(relative);
        }
    }
    let fingerprint = hex(&hasher.finalize());
    Ok(Scanned {
        manifest,
        files,
        strays,
        fingerprint,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest(dir: &Path) -> std::io::Result<String> {
    let mut entries = Vec::new();
    walk(dir, dir, &mut entries)?;
    let mut hasher = Sha256::new();
    for (relative, path, _, _) in entries {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(&path)?);
        hasher.update([0]);
    }
    Ok(hex(&hasher.finalize()))
}

fn limited(value: Option<&str>, max: usize) -> Option<String> {
    value.map(|value| value.chars().take(max).collect())
}

fn decode(input: &SkillFileInput) -> Option<Vec<u8>> {
    match input.encoding() {
        SkillFileEncoding::Utf8 => Some(input.content.clone().into_bytes()),
        SkillFileEncoding::Base64 => base64::engine::general_purpose::STANDARD
            .decode(input.content.as_bytes())
            .ok(),
    }
}

fn file_dtos(files: &[(String, PathBuf, u64)]) -> Vec<SkillFileDto> {
    files
        .iter()
        .map(|(path, _, size)| SkillFileDto {
            path: path.clone(),
            size: *size,
        })
        .collect()
}

fn skill_dto(row: &SkillRow, update: Option<SkillUpdateDto>) -> Result<SkillDto, AppError> {
    let dto = SkillDto {
        id: SkillId::parse(&row.id).ok_or_else(malformed)?,
        key: row.key.clone(),
        name: row.name.clone(),
        description: row.description.clone(),
        enabled: row.enabled,
        source: row.source,
        status: row.status,
        issues: row.issues.clone(),
        file_count: u64::try_from(row.file_count).unwrap_or(0),
        version: row.version.clone(),
        author: row.author.clone(),
        update,
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    Ok(dto)
}

fn status_of(issues: &[SkillIssueDto]) -> SkillStatus {
    if issues.is_empty() {
        SkillStatus::Ready
    } else {
        SkillStatus::Invalid
    }
}

fn copy_key(key: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|attempt| {
            let suffix = if attempt == 1 {
                COPY_SUFFIX.to_owned()
            } else {
                format!("{COPY_SUFFIX}-{attempt}")
            };
            let base: String = key
                .chars()
                .take(SKILL_KEY_MAX.saturating_sub(suffix.len()))
                .collect();
            format!("{}{suffix}", base.trim_end_matches('-'))
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| key.to_owned())
}

fn copy_name(name: &str) -> String {
    let limit = SKILL_NAME_MAX - COPY_NAME_SUFFIX.chars().count();
    let base: String = js_trim(name).chars().take(limit).collect();
    format!("{}{COPY_NAME_SUFFIX}", base.trim_end())
}

fn display_name(manifest: &Manifest, key: &str) -> String {
    manifest
        .display_name
        .clone()
        .filter(|name| !js_trim(name).is_empty())
        .unwrap_or_else(|| key.to_owned())
}

pub const READ_FIRST_FIELD: &str = "readFirst";
pub const REQUIRES_FIELD: &str = "requires";
const READ_FIRST_DIRS: [&str; 2] = ["references/", "assets/"];

fn reference_issues(metadata: &[(String, String)], files: &BTreeSet<&str>) -> Vec<SkillIssueDto> {
    let mut issues = Vec::new();
    let checked = |field: String, path: &str, seen: &mut HashSet<String>, dirs: &[&str]| {
        if !is_skill_path(path) || !dirs.iter().any(|dir| path.starts_with(dir)) {
            Some(issue(field, SkillIssueRule::Path))
        } else if !files.contains(path) {
            Some(issue(field, SkillIssueRule::Missing))
        } else if !seen.insert(path.to_owned()) {
            Some(issue(field, SkillIssueRule::Duplicate))
        } else {
            None
        }
    };
    match read_first(metadata) {
        None => issues.push(issue(READ_FIRST_FIELD, SkillIssueRule::Manifest)),
        Some(paths) => {
            let mut seen = HashSet::new();
            for (index, path) in paths.iter().enumerate() {
                issues.extend(checked(
                    format!("{READ_FIRST_FIELD}.{index}"),
                    path,
                    &mut seen,
                    &READ_FIRST_DIRS,
                ));
            }
        }
    }
    match requires(metadata) {
        None => issues.push(issue(REQUIRES_FIELD, SkillIssueRule::Manifest)),
        Some(entries) => {
            let mut scripts = HashSet::new();
            for (index, (script, needed)) in entries.iter().enumerate() {
                issues.extend(checked(
                    format!("{REQUIRES_FIELD}.{index}"),
                    script,
                    &mut scripts,
                    &[SCRIPTS_PREFIX],
                ));
                let mut seen = HashSet::new();
                for (position, path) in needed.iter().enumerate() {
                    issues.extend(checked(
                        format!("{REQUIRES_FIELD}.{index}.{position}"),
                        path,
                        &mut seen,
                        &["references/", "assets/", SCRIPTS_PREFIX],
                    ));
                }
            }
        }
    }
    issues
}

fn authored_metadata(version: &str, input: &AuthoredSkill) -> Vec<(String, String)> {
    let mut metadata = vec![(VERSION.to_owned(), version.to_owned())];
    if !input.read_first.is_empty() {
        metadata.push((READ_FIRST.to_owned(), json!(input.read_first).to_string()));
    }
    if !input.requires.is_empty() {
        let requires: BTreeMap<&str, &Vec<String>> = input
            .requires
            .iter()
            .map(|(script, files)| (script.as_str(), files))
            .collect();
        metadata.push((REQUIRES.to_owned(), json!(requires).to_string()));
    }
    metadata
}

fn content_issues(
    scanned: &Scanned,
    manifest: Result<&Manifest, ()>,
    key: &str,
) -> Vec<SkillIssueDto> {
    let mut issues = Vec::new();
    match manifest {
        Err(()) => issues.push(issue("instructions", SkillIssueRule::Manifest)),
        Ok(manifest) => {
            if manifest.name != key {
                issues.push(issue("key", SkillIssueRule::Manifest));
            }
            if js_trim(&manifest.description).is_empty() {
                issues.push(issue("description", SkillIssueRule::Required));
            }
            if js_trim(&manifest.instructions).is_empty() {
                issues.push(issue("instructions", SkillIssueRule::Required));
            }
            let files: BTreeSet<&str> = scanned
                .files
                .iter()
                .map(|(path, _, _)| path.as_str())
                .collect();
            issues.extend(reference_issues(&manifest.metadata, &files));
        }
    }
    if !scanned.strays.is_empty() {
        issues.push(issue("files", SkillIssueRule::Path));
    }
    if scanned.files.len() > SKILL_FILES_MAX {
        issues.push(issue("files", SkillIssueRule::TooManyFiles));
    }
    if scanned
        .files
        .iter()
        .any(|(_, _, size)| *size > SKILL_FILE_MAX_BYTES)
    {
        issues.push(issue("files", SkillIssueRule::FileTooLarge));
    }
    if scanned.files.iter().map(|(_, _, size)| *size).sum::<u64>() > SKILL_TOTAL_MAX_BYTES {
        issues.push(issue("files", SkillIssueRule::SkillTooLarge));
    }
    issues
}

impl SkillLibrary {
    pub fn new(options: SkillLibraryOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                database: options.database,
                root: options.root,
                logger: options.logger,
                clock: options.clock,
                bundled: options.bundled,
                writing: Mutex::new(()),
            }),
        }
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    fn now(&self) -> i64 {
        self.inner.clock.now()
    }

    fn rows(&self) -> Result<Vec<SkillRow>, AppError> {
        self.inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).skills.list()?))
    }

    fn find(&self, id: &str) -> Result<SkillRow, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).skills.find_by_id(id)?)
            })?
            .ok_or_else(not_found)
    }

    fn find_key(&self, key: &str) -> Result<Option<SkillRow>, AppError> {
        self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).skills.find_by_key(key)?)
        })
    }

    fn save(&self, row: &SkillRow) -> Result<SkillRow, AppError> {
        self.inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).skills.save(row)?))
    }

    fn read_marker(&self) -> BTreeMap<String, Seeded> {
        let marker = fs::read_to_string(self.inner.root.join(BUNDLED_MARKER))
            .ok()
            .and_then(|text| serde_json::from_str::<Marker>(&text).ok());
        match marker {
            Some(Marker::Installed(seeded)) => seeded,
            Some(Marker::Keys(keys)) => keys
                .into_iter()
                .map(|key| (key, Seeded::default()))
                .collect(),
            None => BTreeMap::new(),
        }
    }

    fn write_marker(&self, seeded: &BTreeMap<String, Seeded>) -> Result<(), AppError> {
        let text = serde_json::to_string(seeded).unwrap_or_else(|_| "{}".to_owned());
        fs::write(self.inner.root.join(BUNDLED_MARKER), text).map_err(|error| storage(&error))
    }

    fn bundled_of(&self, key: &str) -> Option<&'static BundledSkill> {
        self.inner.bundled.iter().find(|skill| skill.key == key)
    }

    fn update_of(
        &self,
        row: &SkillRow,
        seeded: &BTreeMap<String, Seeded>,
    ) -> Option<SkillUpdateDto> {
        if row.source != SkillSource::Bundled {
            return None;
        }
        let version = self.bundled_of(&row.key)?.version()?;
        if !is_newer(&version, row.version.as_deref()) {
            return None;
        }
        let recorded = seeded
            .get(&row.key)
            .and_then(|entry| entry.digest.as_deref());
        let modified = match (recorded, digest(Path::new(&row.path))) {
            (Some(recorded), Ok(current)) => recorded != current,
            _ => true,
        };
        Some(SkillUpdateDto {
            version: limited(Some(&version), SKILL_VERSION_MAX).unwrap_or_default(),
            modified,
        })
    }

    fn dto_with(
        &self,
        row: &SkillRow,
        seeded: &BTreeMap<String, Seeded>,
    ) -> Result<SkillDto, AppError> {
        skill_dto(row, self.update_of(row, seeded))
    }

    fn dto(&self, row: &SkillRow) -> Result<SkillDto, AppError> {
        self.dto_with(row, &self.read_marker())
    }

    fn refresh_with(&self, row: SkillRow) -> Result<SkillRow, AppError> {
        let dir = PathBuf::from(&row.path);
        let scanned = if dir.join(SKILL_FILE).is_file() {
            scan(&dir).ok()
        } else {
            None
        };
        let mut next = row.clone();
        match scanned {
            None => {
                next.issues = vec![issue("path", SkillIssueRule::Missing)];
            }
            Some(scanned) if scanned.fingerprint != row.content_hash => {
                let manifest = scanned
                    .manifest
                    .as_deref()
                    .map(parse_manifest)
                    .and_then(Result::ok);
                if let Some(manifest) = &manifest {
                    next.name = display_name(manifest, &row.key);
                    next.description = js_trim(&manifest.description).to_owned();
                    next.version = limited(manifest.meta(VERSION), SKILL_VERSION_MAX);
                    next.author = limited(manifest.meta(AUTHOR), SKILL_AUTHOR_MAX);
                }
                next.issues = content_issues(&scanned, manifest.as_ref().ok_or(()), &row.key);
                next.file_count = scanned.files.len() as i64;
                next.content_hash = scanned.fingerprint;
                self.inner.logger.log(
                    LogLevel::Info,
                    "skills",
                    "Re-read a skill changed on disk",
                    &[
                        ("skill", json!(row.key)),
                        ("issues", json!(next.issues.len())),
                    ],
                );
            }
            Some(_) => {
                next.issues
                    .retain(|issue| issue.rule != SkillIssueRule::Missing);
            }
        }
        next.status = status_of(&next.issues);
        if next == row {
            return Ok(row);
        }
        next.updated_at = self.now();
        self.save(&next)
    }

    fn refresh(&self, row: SkillRow) -> Result<SkillRow, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.refresh_with(row)
    }

    pub fn reconcile(&self) -> Result<Vec<SkillDto>, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Err(error) = fs::create_dir_all(&self.inner.root) {
            return Err(storage(&error));
        }
        let known: HashSet<String> = self.rows()?.into_iter().map(|row| row.key).collect();
        let entries = fs::read_dir(&self.inner.root).map_err(|error| storage(&error))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with(STAGING_PREFIX) || name.starts_with(TRASH_PREFIX) {
                let _ = fs::remove_dir_all(&path);
                continue;
            }
            if known.contains(&name) || !is_skill_key(&name) || !path.join(SKILL_FILE).is_file() {
                continue;
            }
            let now = self.now();
            self.save(&SkillRow {
                id: new_id().hyphenated().to_string(),
                key: name.clone(),
                name: name.clone(),
                description: String::new(),
                enabled: true,
                source: SkillSource::Local,
                status: SkillStatus::Invalid,
                issues: Vec::new(),
                file_count: 0,
                content_hash: String::new(),
                path: path.to_string_lossy().into_owned(),
                version: None,
                author: None,
                created_at: now,
                updated_at: now,
            })?;
        }
        self.list_unlocked()
    }

    fn list_unlocked(&self) -> Result<Vec<SkillDto>, AppError> {
        let seeded = self.read_marker();
        self.rows()?
            .into_iter()
            .map(|row| self.refresh_with(row))
            .map(|row| self.dto_with(&row?, &seeded))
            .collect()
    }

    pub fn list(&self) -> Result<Vec<SkillDto>, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.list_unlocked()
    }

    pub fn usable(&self) -> Result<Vec<SkillDto>, AppError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|skill| skill.enabled && skill.status == SkillStatus::Ready)
            .collect())
    }

    fn detail(&self, row: &SkillRow) -> Result<SkillDetailDto, AppError> {
        let dir = PathBuf::from(&row.path);
        let (instructions, files) = match scan(&dir) {
            Ok(scanned) => (
                scanned
                    .manifest
                    .as_deref()
                    .and_then(|text| parse_manifest(text).ok())
                    .map(|manifest| manifest.instructions)
                    .unwrap_or_default(),
                file_dtos(&scanned.files),
            ),
            Err(_) => (String::new(), Vec::new()),
        };
        Ok(SkillDetailDto {
            skill: self.dto(row)?,
            instructions,
            files,
        })
    }

    pub fn get(&self, id: &SkillId) -> Result<SkillDetailDto, AppError> {
        let row = self.refresh(self.find(&id.to_string())?)?;
        self.detail(&row)
    }

    pub fn find_by_key(&self, key: &str) -> Result<Option<SkillDto>, AppError> {
        self.find_key(key)?
            .map(|row| self.refresh(row).and_then(|row| self.dto(&row)))
            .transpose()
    }

    fn stage_inputs(files: &[SkillFileInput], field: &str, draft: &mut Draft) {
        for (index, file) in files.iter().enumerate() {
            let field = format!("{field}.{index}");
            let path = js_trim(&file.path).to_owned();
            if !is_skill_path(&path) {
                draft.issues.push(issue(field, SkillIssueRule::Path));
                continue;
            }
            let Some(bytes) = decode(file) else {
                draft.issues.push(issue(field, SkillIssueRule::Encoding));
                continue;
            };
            draft.files.insert(
                path,
                Staged {
                    field,
                    size: bytes.len() as u64,
                    source: Source::Bytes(bytes),
                },
            );
        }
    }

    fn check(&self, draft: &Draft, id: Option<&str>) -> Result<Vec<SkillIssueDto>, AppError> {
        let mut issues = draft.issues.clone();
        if draft.key.is_empty() {
            issues.push(issue("key", SkillIssueRule::Required));
        } else if !is_skill_key(&draft.key) {
            issues.push(issue("key", SkillIssueRule::Pattern));
        } else if self
            .find_key(&draft.key)?
            .is_some_and(|row| Some(row.id.as_str()) != id)
        {
            issues.push(issue("key", SkillIssueRule::Duplicate));
        }
        for (field, value) in [
            ("name", &draft.name),
            ("description", &draft.description),
            ("instructions", &draft.instructions),
        ] {
            if js_trim(value).is_empty() {
                issues.push(issue(field, SkillIssueRule::Required));
            }
        }
        let files: BTreeSet<&str> = draft.files.keys().map(String::as_str).collect();
        issues.extend(reference_issues(&draft.metadata, &files));
        if draft.files.len() > SKILL_FILES_MAX {
            issues.push(issue("files", SkillIssueRule::TooManyFiles));
        }
        for staged in draft.files.values() {
            if staged.size > SKILL_FILE_MAX_BYTES {
                issues.push(issue(staged.field.clone(), SkillIssueRule::FileTooLarge));
            }
        }
        if draft.files.values().map(|staged| staged.size).sum::<u64>() > SKILL_TOTAL_MAX_BYTES {
            issues.push(issue("files", SkillIssueRule::SkillTooLarge));
        }
        Ok(issues)
    }

    fn write_staging(&self, draft: &Draft, staging: &Path) -> std::io::Result<()> {
        fs::create_dir_all(staging)?;
        let manifest = Manifest {
            name: draft.key.clone(),
            description: js_trim(&draft.description).to_owned(),
            display_name: Some(js_trim(&draft.name).to_owned()),
            extra: draft.extra.clone(),
            metadata: draft.metadata.clone(),
            instructions: draft.instructions.clone(),
        };
        fs::write(staging.join(SKILL_FILE), render_manifest(&manifest))?;
        for (path, staged) in &draft.files {
            let target = path
                .split('/')
                .fold(staging.to_path_buf(), |target, segment| {
                    target.join(segment)
                });
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            match &staged.source {
                Source::Disk(source) => {
                    fs::copy(source, &target)?;
                }
                Source::Bytes(bytes) => fs::write(&target, bytes)?,
            }
        }
        Ok(())
    }

    fn swap(&self, staging: &Path, target: &Path, previous: Option<&Path>) -> std::io::Result<()> {
        let previous = previous.filter(|path| path.exists());
        let Some(previous) = previous else {
            return fs::rename(staging, target);
        };
        let trash = self
            .inner
            .root
            .join(format!("{TRASH_PREFIX}{}", new_id().simple()));
        fs::rename(previous, &trash)?;
        if let Err(error) = fs::rename(staging, target) {
            let _ = fs::rename(&trash, previous);
            return Err(error);
        }
        let _ = fs::remove_dir_all(&trash);
        Ok(())
    }

    fn commit(
        &self,
        existing: Option<&SkillRow>,
        draft: Draft,
    ) -> Result<SkillDetailDto, AppError> {
        let id = existing.map(|row| row.id.clone());
        let issues = self.check(&draft, id.as_deref())?;
        if !issues.is_empty() {
            return Err(rejected(&issues));
        }
        let target = self.inner.root.join(&draft.key);
        let previous = existing.map(|row| PathBuf::from(&row.path));
        if target.exists() && previous.as_deref() != Some(target.as_path()) {
            return Err(rejected(&[issue("key", SkillIssueRule::Duplicate)]));
        }
        fs::create_dir_all(&self.inner.root).map_err(|error| storage(&error))?;
        let staging = self
            .inner
            .root
            .join(format!("{STAGING_PREFIX}{}", new_id().simple()));
        let written = self
            .write_staging(&draft, &staging)
            .and_then(|()| self.swap(&staging, &target, previous.as_deref()));
        if let Err(error) = written {
            let _ = fs::remove_dir_all(&staging);
            return Err(storage(&error));
        }
        let scanned = scan(&target).map_err(|error| storage(&error))?;
        let now = self.now();
        let row = self.save(&SkillRow {
            id: id.unwrap_or_else(|| new_id().hyphenated().to_string()),
            key: draft.key.clone(),
            name: js_trim(&draft.name).to_owned(),
            description: js_trim(&draft.description).to_owned(),
            enabled: draft.enabled,
            source: draft.source,
            status: status_of(&issues),
            issues,
            file_count: scanned.files.len() as i64,
            content_hash: scanned.fingerprint,
            path: target.to_string_lossy().into_owned(),
            version: limited(meta(&draft.metadata, VERSION), SKILL_VERSION_MAX),
            author: limited(meta(&draft.metadata, AUTHOR), SKILL_AUTHOR_MAX),
            created_at: existing.map_or(now, |row| row.created_at),
            updated_at: now,
        })?;
        self.inner.logger.log(
            LogLevel::Info,
            "skills",
            "Saved a skill",
            &[("skill", json!(row.key)), ("files", json!(row.file_count))],
        );
        self.detail(&row)
    }

    pub fn create(&self, input: &CreateSkillInput) -> Result<SkillDetailDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut draft = Draft {
            key: js_trim(&input.key).to_owned(),
            name: input.name.clone(),
            description: input.description.clone(),
            instructions: input.instructions.trim().to_owned(),
            enabled: input.enabled.unwrap_or(true),
            source: SkillSource::Local,
            extra: Vec::new(),
            metadata: Vec::new(),
            files: BTreeMap::new(),
            issues: Vec::new(),
        };
        Self::stage_inputs(
            input.files.as_deref().unwrap_or_default(),
            "files",
            &mut draft,
        );
        self.commit(None, draft)
    }

    fn draft_of(row: &SkillRow) -> Result<Draft, AppError> {
        let dir = PathBuf::from(&row.path);
        let scanned = scan(&dir).map_err(|error| storage(&error))?;
        let manifest = scanned
            .manifest
            .as_deref()
            .and_then(|text| parse_manifest(text).ok())
            .unwrap_or_default();
        let files = scanned
            .files
            .into_iter()
            .map(|(path, source, size)| {
                (
                    path,
                    Staged {
                        field: "files".to_owned(),
                        size,
                        source: Source::Disk(source),
                    },
                )
            })
            .collect();
        Ok(Draft {
            key: row.key.clone(),
            name: row.name.clone(),
            description: row.description.clone(),
            instructions: manifest.instructions,
            enabled: row.enabled,
            source: row.source,
            extra: manifest.extra,
            metadata: manifest.metadata,
            files,
            issues: Vec::new(),
        })
    }

    pub fn update(&self, input: &UpdateSkillInput) -> Result<SkillDetailDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let row = self.find(&input.id.to_string())?;
        let mut draft = Self::draft_of(&row)?;
        if let Some(key) = &input.key {
            js_trim(key).clone_into(&mut draft.key);
        }
        if let Some(name) = &input.name {
            draft.name.clone_from(name);
        }
        if let Some(description) = &input.description {
            draft.description.clone_from(description);
        }
        if let Some(instructions) = &input.instructions {
            instructions.trim().clone_into(&mut draft.instructions);
        }
        if let Some(enabled) = input.enabled {
            draft.enabled = enabled;
        }
        for (index, path) in input.remove_files.iter().flatten().enumerate() {
            if draft.files.remove(js_trim(path)).is_none() {
                draft.issues.push(issue(
                    format!("removeFiles.{index}"),
                    SkillIssueRule::Missing,
                ));
            }
        }
        Self::stage_inputs(
            input.files.as_deref().unwrap_or_default(),
            "files",
            &mut draft,
        );
        self.commit(Some(&row), draft)
    }

    pub fn set_enabled(&self, id: &SkillId, enabled: bool) -> Result<SkillDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let row = self.find(&id.to_string())?;
        let saved = if row.enabled == enabled {
            row
        } else {
            self.save(&SkillRow {
                enabled,
                updated_at: self.now(),
                ..row
            })?
        };
        self.dto(&self.refresh_with(saved)?)
    }

    pub fn author(&self, input: &AuthoredSkill) -> Result<SkillDetailDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let key = js_trim(&input.key).to_owned();
        let existing = if key.is_empty() {
            None
        } else {
            self.find_key(&key)?
        };
        if let Some(row) = &existing {
            if !input.replace {
                return Err(rejected(&[issue("key", SkillIssueRule::Duplicate)]));
            }
            if row.source == SkillSource::Bundled {
                return Err(AppError::new(
                    AppErrorCode::Conflict,
                    SKILL_BUNDLED_PROTECTED,
                ));
            }
        }
        let version = input
            .version
            .as_deref()
            .map(js_trim)
            .filter(|version| !version.is_empty())
            .unwrap_or(AUTHORED_VERSION);
        let mut draft = Draft {
            key,
            name: input.name.clone(),
            description: input.description.clone(),
            instructions: input.instructions.trim().to_owned(),
            enabled: existing.as_ref().is_none_or(|row| row.enabled),
            source: SkillSource::Local,
            extra: Vec::new(),
            metadata: authored_metadata(version, input),
            files: BTreeMap::new(),
            issues: Vec::new(),
        };
        let files: Vec<SkillFileInput> = input
            .files
            .iter()
            .map(|(path, content)| SkillFileInput {
                path: path.clone(),
                content: content.clone(),
                encoding: None,
            })
            .collect();
        Self::stage_inputs(&files, "files", &mut draft);
        self.commit(existing.as_ref(), draft)
    }

    pub fn duplicate(&self, id: &SkillId) -> Result<SkillDetailDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let row = self.find(&id.to_string())?;
        let mut draft = Self::draft_of(&row)?;
        let taken: HashSet<String> = self.rows()?.into_iter().map(|row| row.key).collect();
        draft.key = copy_key(&row.key, |candidate| {
            taken.contains(candidate) || self.inner.root.join(candidate).exists()
        });
        draft.name = copy_name(&row.name);
        draft.source = SkillSource::Local;
        self.commit(None, draft)
    }

    fn bundled_draft(skill: &BundledSkill) -> Result<Draft, AppError> {
        let manifest = parse_manifest(skill.manifest)
            .map_err(|_| rejected(&[issue("instructions", SkillIssueRule::Manifest)]))?;
        let files = skill
            .files
            .iter()
            .map(|file| {
                (
                    file.path.to_owned(),
                    Staged {
                        field: "files".to_owned(),
                        size: file.content.len() as u64,
                        source: Source::Bytes(file.content.as_bytes().to_vec()),
                    },
                )
            })
            .collect();
        Ok(Draft {
            name: display_name(&manifest, skill.key),
            key: skill.key.to_owned(),
            description: manifest.description.clone(),
            instructions: manifest.instructions.trim().to_owned(),
            enabled: true,
            source: SkillSource::Bundled,
            extra: manifest.extra.clone(),
            metadata: manifest.metadata,
            files,
            issues: Vec::new(),
        })
    }

    fn seeded_entry(skill: &BundledSkill, dir: &Path) -> Seeded {
        Seeded {
            version: skill.version(),
            digest: digest(dir).ok(),
        }
    }

    pub fn install_bundled(&self) -> Result<Vec<String>, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        fs::create_dir_all(&self.inner.root).map_err(|error| storage(&error))?;
        let mut seeded = self.read_marker();
        let mut installed = Vec::new();
        for skill in self.inner.bundled {
            if seeded.contains_key(skill.key) {
                continue;
            }
            let taken =
                self.find_key(skill.key)?.is_some() || self.inner.root.join(skill.key).exists();
            let mut entry = Seeded::default();
            if !taken {
                self.commit(None, Self::bundled_draft(skill)?)?;
                entry = Self::seeded_entry(skill, &self.inner.root.join(skill.key));
                installed.push(skill.key.to_owned());
            }
            seeded.insert(skill.key.to_owned(), entry);
        }
        self.write_marker(&seeded)?;
        if !installed.is_empty() {
            self.inner.logger.log(
                LogLevel::Info,
                "skills",
                "Installed bundled skills",
                &[("skills", json!(installed))],
            );
        }
        Ok(installed)
    }

    fn apply_update(
        &self,
        row: &SkillRow,
        seeded: &mut BTreeMap<String, Seeded>,
    ) -> Result<SkillDetailDto, AppError> {
        let skill = self
            .update_of(row, seeded)
            .and_then(|_| self.bundled_of(&row.key))
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, SKILL_UP_TO_DATE))?;
        let mut draft = Self::bundled_draft(skill)?;
        draft.enabled = row.enabled;
        let detail = self.commit(Some(row), draft)?;
        seeded.insert(
            row.key.clone(),
            Self::seeded_entry(skill, &self.inner.root.join(&row.key)),
        );
        self.inner.logger.log(
            LogLevel::Info,
            "skills",
            "Updated a bundled skill",
            &[
                ("skill", json!(row.key)),
                ("from", json!(row.version)),
                ("to", json!(detail.skill.version)),
            ],
        );
        Ok(detail)
    }

    pub fn update_bundled(&self, id: &SkillId) -> Result<SkillDetailDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let row = self.find(&id.to_string())?;
        let mut seeded = self.read_marker();
        let detail = self.apply_update(&row, &mut seeded)?;
        self.write_marker(&seeded)?;
        Ok(detail)
    }

    pub fn update_all_bundled(&self) -> Result<Vec<SkillDto>, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut seeded = self.read_marker();
        let mut failure = None;
        for row in self.rows()? {
            if self.update_of(&row, &seeded).is_none() {
                continue;
            }
            if let Err(error) = self.apply_update(&row, &mut seeded) {
                failure.get_or_insert(error);
            }
        }
        self.write_marker(&seeded)?;
        match failure {
            Some(error) => Err(error),
            None => self.list_unlocked(),
        }
    }

    pub fn remove(&self, id: &SkillId) -> Result<(), AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let row = self.find(&id.to_string())?;
        let dir = PathBuf::from(&row.path);
        if dir.exists() {
            let trash = self
                .inner
                .root
                .join(format!("{TRASH_PREFIX}{}", new_id().simple()));
            fs::rename(&dir, &trash).map_err(|error| storage(&error))?;
            let _ = fs::remove_dir_all(&trash);
        }
        self.inner.database.with_connection(|connection| {
            Repositories::new(connection).skills.remove(&row.id)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn import(&self, folder: &Path) -> Result<SkillDetailDto, AppError> {
        let _guard = self
            .inner
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !folder.join(SKILL_FILE).is_file() {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                SKILL_IMPORT_NO_MANIFEST,
            ));
        }
        let scanned = scan(folder).map_err(|error| storage(&error))?;
        let manifest = scanned
            .manifest
            .as_deref()
            .map(parse_manifest)
            .and_then(Result::ok)
            .ok_or_else(|| rejected(&[issue("instructions", SkillIssueRule::Manifest)]))?;
        let folder_name = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let key = if is_skill_key(&manifest.name) {
            manifest.name.clone()
        } else {
            folder_name
        };
        let mut files: BTreeMap<String, Staged> = scanned
            .files
            .into_iter()
            .map(|(path, source, size)| {
                (
                    path,
                    Staged {
                        field: "files".to_owned(),
                        size,
                        source: Source::Disk(source),
                    },
                )
            })
            .collect();
        for stray in scanned.strays {
            let moved = format!("references/{stray}");
            if is_skill_path(&moved) && !files.contains_key(&moved) {
                let source = stray
                    .split('/')
                    .fold(folder.to_path_buf(), |path, segment| path.join(segment));
                let size = fs::metadata(&source).map_or(0, |metadata| metadata.len());
                files.insert(
                    moved,
                    Staged {
                        field: "files".to_owned(),
                        size,
                        source: Source::Disk(source),
                    },
                );
            }
        }
        let draft = Draft {
            name: display_name(&manifest, &key),
            key,
            description: manifest.description.clone(),
            instructions: manifest.instructions.clone(),
            enabled: true,
            source: SkillSource::Imported,
            extra: manifest.extra.clone(),
            metadata: manifest.metadata,
            files,
            issues: Vec::new(),
        };
        self.commit(None, draft)
    }

    pub fn export(&self, id: &SkillId, destination: &Path) -> Result<PathBuf, AppError> {
        let row = self.find(&id.to_string())?;
        let source = PathBuf::from(&row.path);
        let scanned = scan(&source).map_err(|error| storage(&error))?;
        let target = destination.join(&row.key);
        if target.exists() {
            return Err(AppError::new(AppErrorCode::Conflict, SKILL_EXPORT_TAKEN));
        }
        let copied = (|| -> std::io::Result<()> {
            fs::create_dir_all(&target)?;
            fs::copy(source.join(SKILL_FILE), target.join(SKILL_FILE))?;
            for (path, file, _) in &scanned.files {
                let into = path
                    .split('/')
                    .fold(target.clone(), |into, segment| into.join(segment));
                if let Some(parent) = into.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(file, into)?;
            }
            Ok(())
        })();
        if let Err(error) = copied {
            let _ = fs::remove_dir_all(&target);
            return Err(storage(&error));
        }
        Ok(target)
    }

    fn usable_row(&self, key: &str) -> Result<SkillRow, AppError> {
        let row = self.find_key(js_trim(key))?.ok_or_else(not_found)?;
        let row = self.refresh(row)?;
        if !row.enabled {
            return Err(AppError::new(AppErrorCode::Conflict, SKILL_DISABLED));
        }
        if row.status != SkillStatus::Ready {
            return Err(AppError::new(AppErrorCode::Conflict, SKILL_NOT_READY));
        }
        Ok(row)
    }

    pub fn load(&self, key: &str) -> Result<LoadedSkill, AppError> {
        let row = self.usable_row(key)?;
        let scanned = scan(Path::new(&row.path)).map_err(|error| storage(&error))?;
        let manifest = scanned
            .manifest
            .as_deref()
            .and_then(|text| parse_manifest(text).ok())
            .unwrap_or_default();
        Ok(LoadedSkill {
            skill: self.dto(&row)?,
            read_first: read_first(&manifest.metadata).unwrap_or_default(),
            requires: requires(&manifest.metadata).unwrap_or_default(),
            instructions: manifest.instructions,
            files: scanned
                .files
                .iter()
                .map(|(path, file, size)| LoadedFile {
                    purpose: purpose_of(path, file),
                    path: path.clone(),
                    size: *size,
                })
                .collect(),
        })
    }

    pub fn load_by_id(&self, id: &SkillId) -> Result<LoadedSkill, AppError> {
        let row = self.find(&id.to_string())?;
        self.load(&row.key)
    }

    pub fn script(&self, key: &str, path: &str) -> Result<SkillScriptFile, AppError> {
        let path = js_trim(path);
        if !path.starts_with(SCRIPTS_PREFIX) {
            return Err(AppError::new(
                AppErrorCode::PermissionDenied,
                SKILL_SCRIPT_OUTSIDE,
            ));
        }
        let (dir, file) = self.locate(key, path)?;
        Ok(SkillScriptFile {
            path: path.to_owned(),
            dir,
            file,
        })
    }

    pub fn folder(&self, key: &str) -> Result<PathBuf, AppError> {
        let row = self.usable_row(key)?;
        let dir = PathBuf::from(&row.path);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(not_found())
        }
    }

    fn locate(&self, key: &str, path: &str) -> Result<(PathBuf, PathBuf), AppError> {
        let row = self.usable_row(key)?;
        if !is_skill_path(path) {
            return Err(AppError::new(AppErrorCode::NotFound, SKILL_FILE_NOT_FOUND));
        }
        let dir = PathBuf::from(&row.path);
        let file = path
            .split('/')
            .fold(dir.clone(), |file, segment| file.join(segment));
        match (file.canonicalize(), dir.canonicalize()) {
            (Ok(file), Ok(dir)) if file.starts_with(&dir) && file.is_file() => Ok((dir, file)),
            _ => Err(AppError::new(AppErrorCode::NotFound, SKILL_FILE_NOT_FOUND)),
        }
    }

    pub fn read_file(&self, id: &SkillId, path: &str) -> Result<SkillFileContent, AppError> {
        let row = self.find(&id.to_string())?;
        let path = js_trim(path);
        if !is_skill_path(path) {
            return Err(AppError::new(AppErrorCode::NotFound, SKILL_FILE_NOT_FOUND));
        }
        let dir = PathBuf::from(&row.path);
        let file = path
            .split('/')
            .fold(dir.clone(), |file, segment| file.join(segment));
        match (file.canonicalize(), dir.canonicalize()) {
            (Ok(file), Ok(dir)) if file.starts_with(&dir) && file.is_file() => {
                Self::content_of(path, &file)
            }
            _ => Err(AppError::new(AppErrorCode::NotFound, SKILL_FILE_NOT_FOUND)),
        }
    }

    pub fn read(&self, key: &str, path: &str) -> Result<SkillFileContent, AppError> {
        let path = js_trim(path);
        let (_, file) = self.locate(key, path)?;
        Self::content_of(path, &file)
    }

    fn content_of(path: &str, file: &Path) -> Result<SkillFileContent, AppError> {
        let size = fs::metadata(file).map_err(|error| storage(&error))?.len();
        if size > SKILL_FILE_MAX_BYTES {
            return Err(AppError::new(
                AppErrorCode::LimitExceeded,
                SKILL_FILE_TOO_LARGE,
            ));
        }
        let bytes = fs::read(file).map_err(|error| storage(&error))?;
        let text = String::from_utf8(bytes)
            .ok()
            .filter(|text| !text.contains('\u{0}'));
        Ok(SkillFileContent {
            path: path.to_owned(),
            size,
            text,
        })
    }
}
