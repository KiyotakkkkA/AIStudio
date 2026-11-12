use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zvs_contract::{
    AllowRuleScope, AppError, AppErrorCode, POLICY_RULE_ID_MAX, POLICY_RULE_TEXT_MAX,
    PolicyRuleEffect, ProcessPlanDto, USER_RULES_MAX, UserRuleDto, UserRulesDto,
};

use super::action::{FileAction, FileOperation, ReviewedAction};
use super::integrity::{PackKey, mac_file, sign, signed};
use super::rules::{
    ANY_REST, PackOrigin, RuleEffect, RuleOutcome, RuleSet, compile_pack, pack_files, pack_name,
};
use crate::commands::script::interpreter_program;
use crate::platform::ids::new_id;

pub const USER_PACK_FILE: &str = "user.json";
pub const USER_PACK_ID: &str = "user";
pub const LEGACY_PACK_FILE: &str = "user.cli.json";
const UNCONFIRMED_STEM: &str = "user.unconfirmed";
pub const NOTHING_TO_ALLOW: &str = "В запросе нет действия, для которого создаётся правило";
pub const OUTSIDE_FOLDER: &str = "Действие выходит за пределы рабочей папки";
pub const NO_WORKING_FOLDER: &str = "У диалога нет рабочей папки";
pub const PATTERN_CHARACTERS: &str = "Путь содержит символы, которые правило не может выразить";
pub const SCRIPT_RULE: &str =
    "Для интерпретатора или строки сценария дополнительные разрешения не нужны.";
pub const SOURCE_RULE: &str = "Для запуска сценария дополнительные разрешения не нужны.";
pub const VENDOR_RULE: &str = "Встроенные правила требуют подтверждать это действие каждый раз";
pub const PACK_DAMAGED: &str = "Файл правил повреждён.";
pub const PACK_NOT_SAVED: &str = "Не удалось сохранить правило";
pub const RULE_MISSING: &str = "Правило не найдено";
pub const PACK_MISSING: &str = "Файл правил не найден";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilesSpec {
    paths: Vec<String>,
    operations: Vec<FileOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSpec {
    argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRule {
    id: String,
    effect: RuleEffect,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    files: Option<FilesSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    process: Option<ProcessSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<i64>,
}

impl StoredRule {
    fn same_matcher(&self, other: &Self) -> bool {
        self.files == other.files && self.process == other.process
    }

    pub fn pattern(&self) -> Option<Vec<String>> {
        self.process.as_ref().map(|process| process.argv.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPack {
    id: String,
    allows: bool,
    rules: Vec<StoredRule>,
}

impl Default for StoredPack {
    fn default() -> Self {
        Self {
            id: USER_PACK_ID.to_owned(),
            allows: true,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedRule {
    id: String,
    effect: RuleEffect,
    text: String,
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ListedPack {
    rules: Vec<ListedRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenRule {
    pub id: String,
    pub text: String,
}

fn normalised(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_owned()
}

fn literal(path: &str) -> Result<String, AppError> {
    if path.contains(['*', '?', '{', '}', ',']) {
        return Err(refused(PATTERN_CHARACTERS));
    }
    Ok(normalised(path))
}

fn inside(folder: &str, path: &str) -> bool {
    let folder = folder.to_lowercase();
    let path = path.to_lowercase();
    path == folder
        || path
            .strip_prefix(&folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn refused(message: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
}

fn storage(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, PACK_NOT_SAVED).with_detail("reason", error.to_string())
}

fn damaged() -> AppError {
    AppError::new(AppErrorCode::Conflict, PACK_DAMAGED)
}

const fn operation_name(operation: FileOperation) -> &'static str {
    match operation {
        FileOperation::Create => "создание",
        FileOperation::Overwrite => "перезапись",
        FileOperation::Move => "перемещение",
        FileOperation::MoveOnto => "перемещение с заменой",
        FileOperation::Delete => "удаление",
    }
}

const fn wire_effect(effect: RuleEffect) -> PolicyRuleEffect {
    match effect {
        RuleEffect::Allow => PolicyRuleEffect::Allow,
        RuleEffect::Review => PolicyRuleEffect::Review,
        RuleEffect::Ask => PolicyRuleEffect::Ask,
        RuleEffect::Deny => PolicyRuleEffect::Deny,
    }
}

fn working_folder(folder: Option<&str>) -> Result<String, AppError> {
    literal(folder.ok_or_else(|| refused(NO_WORKING_FOLDER))?)
}

fn files_rule(
    actions: &[FileAction],
    scope: AllowRuleScope,
    folder: Option<&str>,
) -> Result<FilesSpec, AppError> {
    let destructive: Vec<&FileAction> = actions
        .iter()
        .filter(|action| action.operation.destructive())
        .collect();
    if destructive.is_empty() {
        return Err(refused(NOTHING_TO_ALLOW));
    }
    let mut operations: Vec<FileOperation> = Vec::new();
    for action in &destructive {
        if !operations.contains(&action.operation) {
            operations.push(action.operation);
        }
    }
    let touched = || {
        destructive
            .iter()
            .flat_map(|action| std::iter::once(action.path.as_str()).chain(action.from.as_deref()))
    };
    let paths = match scope {
        AllowRuleScope::Folder => {
            let folder = working_folder(folder)?;
            if touched().any(|path| !inside(&folder, &normalised(path))) {
                return Err(refused(OUTSIDE_FOLDER));
            }
            vec![format!("{folder}/**")]
        }
        AllowRuleScope::Anywhere => {
            let mut paths: Vec<String> = Vec::new();
            for path in touched() {
                let path = literal(path)?;
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            paths
        }
    };
    Ok(FilesSpec { paths, operations })
}

fn word(argument: &str) -> bool {
    let mut chars = argument.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && chars.all(|next| next.is_ascii_alphanumeric() || matches!(next, '_' | '-'))
}

fn program_name(program: &str) -> String {
    let name = normalised(program);
    let name = name.rsplit('/').next().unwrap_or_default().to_lowercase();
    for extension in [".exe", ".cmd", ".bat", ".com"] {
        if let Some(stem) = name.strip_suffix(extension) {
            return stem.to_owned();
        }
    }
    name
}

fn process_rule(
    process: &ProcessPlanDto,
    scope: AllowRuleScope,
    folder: Option<&str>,
) -> Result<ProcessSpec, AppError> {
    if process.source.is_some() || process.language.is_some() {
        return Err(refused(SOURCE_RULE));
    }
    let Some((program, rest)) = process.argv.split_first() else {
        return Err(refused(NOTHING_TO_ALLOW));
    };
    if process.script_string || interpreter_program(program) {
        return Err(refused(SCRIPT_RULE));
    }
    let mut pattern = vec![literal(&program_name(program))?];
    pattern.extend(rest.iter().take_while(|argument| word(argument)).cloned());
    pattern.push(ANY_REST.to_owned());
    let cwd = match scope {
        AllowRuleScope::Folder => {
            let folder = working_folder(folder)?;
            if !inside(&folder, &normalised(&process.cwd)) {
                return Err(refused(OUTSIDE_FOLDER));
            }
            Some(format!("{{{folder},{folder}/**}}"))
        }
        AllowRuleScope::Anywhere => None,
    };
    Ok(ProcessSpec { argv: pattern, cwd })
}

fn describe(rule: &StoredRule, scope: AllowRuleScope, folder: Option<&str>) -> String {
    if let Some(files) = &rule.files {
        let operations: Vec<&str> = files
            .operations
            .iter()
            .map(|operation| operation_name(*operation))
            .collect();
        return format!(
            "Разрешено из подтверждения: {} — {}",
            operations.join(", "),
            files.paths.join(", ")
        );
    }
    let argv = rule
        .process
        .as_ref()
        .map(|process| process.argv.join(" "))
        .unwrap_or_default();
    let place = match (scope, folder) {
        (AllowRuleScope::Folder, Some(folder)) => format!("в папке {}", normalised(folder)),
        _ => "в любой папке".to_owned(),
    };
    format!("Разрешено из подтверждения: `{argv}` {place}")
}

pub fn derive(
    action: &ReviewedAction,
    scope: AllowRuleScope,
    folder: Option<&str>,
    vendor: &RuleSet,
    now: i64,
) -> Result<StoredRule, AppError> {
    let mut rule = StoredRule {
        id: format!("{USER_PACK_ID}.{}", new_id().simple()),
        effect: RuleEffect::Allow,
        text: String::new(),
        files: None,
        process: None,
        created_at: Some(now),
    };
    match action {
        ReviewedAction::Files { actions, .. } => {
            rule.files = Some(files_rule(actions, scope, folder)?);
        }
        ReviewedAction::Process { process, .. } => {
            rule.process = Some(process_rule(process, scope, folder)?);
        }
        _ => return Err(refused(NOTHING_TO_ALLOW)),
    }
    if matches!(
        vendor.evaluate(action),
        RuleOutcome::Ask { .. } | RuleOutcome::Deny { .. }
    ) || vendor.denied(action).is_some()
    {
        return Err(refused(VENDOR_RULE));
    }
    rule.text = describe(&rule, scope, folder);
    Ok(rule)
}

fn read(file: &Path) -> Result<Option<String>, AppError> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage(&error)),
    }
}

fn write(file: &Path, text: &str) -> Result<(), AppError> {
    compile_pack(text, PackOrigin::User).map_err(|reason| {
        AppError::new(AppErrorCode::Unknown, PACK_NOT_SAVED).with_detail("reason", reason)
    })?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| storage(&error))?;
    }
    let staged = file.with_extension("json.tmp");
    std::fs::write(&staged, text).map_err(|error| storage(&error))?;
    std::fs::rename(&staged, file).map_err(|error| storage(&error))
}

fn save(file: &Path, pack: &StoredPack, key: &PackKey) -> Result<(), AppError> {
    let text = serde_json::to_string_pretty(pack)
        .map(|text| format!("{text}\n"))
        .map_err(|error| storage(&std::io::Error::other(error)))?;
    write(file, &text)?;
    sign(file, text.as_bytes(), key).map_err(|error| storage(&error))
}

fn free_name(folder: &Path) -> PathBuf {
    let first = folder.join(format!("{UNCONFIRMED_STEM}.json"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|index| folder.join(format!("{UNCONFIRMED_STEM}-{index}.json")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

fn set_aside(folder: &Path, file: &Path) -> Result<String, AppError> {
    let target = free_name(folder);
    std::fs::rename(file, &target).map_err(|error| storage(&error))?;
    let _ = std::fs::remove_file(mac_file(file));
    Ok(pack_name(&target))
}

pub fn migrate(folder: &Path) -> Result<Option<String>, AppError> {
    let legacy = folder.join(LEGACY_PACK_FILE);
    if !legacy.is_file() {
        return Ok(None);
    }
    let target = folder.join(USER_PACK_FILE);
    if target.exists() {
        return set_aside(folder, &legacy).map(Some);
    }
    std::fs::rename(&legacy, &target).map_err(|error| storage(&error))?;
    Ok(Some(USER_PACK_FILE.to_owned()))
}

pub struct Allowed {
    pub rule: WrittenRule,
    pub set_aside: Option<String>,
}

pub fn allow(folder: &Path, key: &PackKey, rule: StoredRule) -> Result<Allowed, AppError> {
    let file = folder.join(USER_PACK_FILE);
    let mut aside = None;
    let mut pack = match read(&file)? {
        None => StoredPack::default(),
        Some(text) if signed(&file, text.as_bytes(), Some(key)) => {
            compile_pack(&text, PackOrigin::User).map_err(|_| damaged())?;
            serde_json::from_str(&text).map_err(|_| damaged())?
        }
        Some(_) => {
            aside = Some(set_aside(folder, &file)?);
            StoredPack::default()
        }
    };
    if let Some(existing) = pack
        .rules
        .iter()
        .find(|known| known.effect == RuleEffect::Allow && known.same_matcher(&rule))
    {
        return Ok(Allowed {
            rule: WrittenRule {
                id: existing.id.clone(),
                text: existing.text.clone(),
            },
            set_aside: aside,
        });
    }
    let written = WrittenRule {
        id: rule.id.clone(),
        text: rule.text.clone(),
    };
    pack.rules.push(rule);
    save(&file, &pack, key)?;
    Ok(Allowed {
        rule: written,
        set_aside: aside,
    })
}

fn listed(rule: ListedRule, pack: &str, trusted: bool) -> Option<UserRuleDto> {
    let id = rule.id.trim();
    let text: String = rule
        .text
        .trim()
        .chars()
        .take(POLICY_RULE_TEXT_MAX)
        .collect();
    if id.is_empty() || id.chars().count() > POLICY_RULE_ID_MAX || text.is_empty() {
        return None;
    }
    Some(UserRuleDto {
        id: id.to_owned(),
        text,
        effect: wire_effect(rule.effect),
        pack: pack.to_owned(),
        created_at: rule.created_at.filter(|created| *created >= 0),
        active: trusted || rule.effect != RuleEffect::Allow,
    })
}

pub fn list(folder: &Path, key: Option<&PackKey>) -> UserRulesDto {
    let mut rules = Vec::new();
    let mut unconfirmed = Vec::new();
    for file in pack_files(Some(folder)) {
        let name = pack_name(&file);
        if name.starts_with('.') {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if compile_pack(&text, PackOrigin::User).is_err() {
            continue;
        }
        let Ok(pack) = serde_json::from_str::<ListedPack>(&text) else {
            continue;
        };
        let trusted = signed(&file, text.as_bytes(), key);
        if !trusted
            && pack
                .rules
                .iter()
                .any(|rule| rule.effect == RuleEffect::Allow)
        {
            unconfirmed.push(name.clone());
        }
        rules.extend(
            pack.rules
                .into_iter()
                .filter_map(|rule| listed(rule, &name, trusted)),
        );
    }
    rules.truncate(USER_RULES_MAX);
    UserRulesDto { rules, unconfirmed }
}

fn rules_of(value: &mut Value) -> Option<&mut Vec<Value>> {
    value.get_mut("rules").and_then(Value::as_array_mut)
}

pub fn remove(folder: &Path, key: Option<&PackKey>, id: &str) -> Result<String, AppError> {
    for file in pack_files(Some(folder)) {
        let Some(text) = read(&file)? else {
            continue;
        };
        let Ok(mut pack) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(rules) = rules_of(&mut pack) else {
            continue;
        };
        let before = rules.len();
        rules.retain(|rule| rule.get("id").and_then(Value::as_str) != Some(id));
        if rules.len() == before {
            continue;
        }
        let trusted = key.filter(|key| signed(&file, text.as_bytes(), Some(key)));
        let text = serde_json::to_string_pretty(&pack)
            .map(|text| format!("{text}\n"))
            .map_err(|error| storage(&std::io::Error::other(error)))?;
        write(&file, &text)?;
        if let Some(key) = trusted {
            sign(&file, text.as_bytes(), key).map_err(|error| storage(&error))?;
        }
        return Ok(pack_name(&file));
    }
    Err(AppError::new(AppErrorCode::NotFound, RULE_MISSING))
}

pub fn confirm(folder: &Path, key: &PackKey, pack: &str) -> Result<(), AppError> {
    let Some(file) = pack_files(Some(folder))
        .into_iter()
        .find(|file| pack_name(file) == pack)
    else {
        return Err(AppError::new(AppErrorCode::NotFound, PACK_MISSING));
    };
    let text = read(&file)?.ok_or_else(|| AppError::new(AppErrorCode::NotFound, PACK_MISSING))?;
    compile_pack(&text, PackOrigin::User).map_err(|_| damaged())?;
    sign(&file, text.as_bytes(), key).map_err(|error| storage(&error))
}

#[cfg(test)]
mod tests {
    use zvs_contract::{CodeLanguage, FileChangeKind, FileDiffDto};

    use super::super::rules::VENDOR_PACK;
    use super::*;

    const WORK: &str = r"C:\work";
    const NOW: i64 = 1_700_000_000_000;
    const KEY: PackKey = PackKey::new([5; 32]);

    fn plan(argv: &[&str], cwd: &str) -> ProcessPlanDto {
        let argv: Vec<String> = argv.iter().map(|part| (*part).to_owned()).collect();
        ProcessPlanDto {
            program: argv[0].clone(),
            language: None,
            script_string: crate::commands::script::script_string(&argv),
            argv,
            cwd: cwd.to_owned(),
            source: None,
            packages: Vec::new(),
            background: false,
            reason: None,
        }
    }

    fn process(argv: &[&str], cwd: &str) -> ReviewedAction {
        ReviewedAction::Process {
            tool: "run_command".to_owned(),
            process: plan(argv, cwd),
        }
    }

    fn changing(path: &str, kind: FileChangeKind) -> ReviewedAction {
        ReviewedAction::files(
            vec![FileDiffDto {
                path: path.to_owned(),
                from: None,
                kind,
                added: 0,
                removed: 1,
                diff: String::new(),
                truncated: false,
            }],
            |_| true,
        )
    }

    fn vendor() -> RuleSet {
        RuleSet::load(VENDOR_PACK, None, None).0
    }

    fn derived(action: &ReviewedAction, scope: AllowRuleScope) -> Result<StoredRule, AppError> {
        derive(action, scope, Some(WORK), &vendor(), NOW)
    }

    fn refusal(action: &ReviewedAction, scope: AllowRuleScope) -> String {
        let error = derived(action, scope).expect_err("refused");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
        error.message
    }

    fn written(packs: &Path, argv: &[&str]) -> WrittenRule {
        let rule = derived(&process(argv, WORK), AllowRuleScope::Anywhere).expect("derived");
        allow(packs, &KEY, rule).expect("written").rule
    }

    fn evaluate(packs: &Path, action: &ReviewedAction) -> RuleOutcome {
        let (rules, problems) = RuleSet::load(VENDOR_PACK, Some(packs), Some(&KEY));
        assert!(problems.is_empty(), "{problems:?}");
        rules.evaluate(action)
    }

    fn allowed(packs: &Path, action: &ReviewedAction) -> bool {
        matches!(evaluate(packs, action), RuleOutcome::Allow { rule, .. } if !rule.starts_with("vendor."))
    }

    fn unconfirmed(packs: &Path) -> Vec<String> {
        list(packs, Some(&KEY)).unconfirmed
    }

    #[test]
    fn a_pattern_keeps_every_leading_word_up_to_a_flag_or_a_path() {
        let cases: [(&[&str], &[&str]); 6] = [
            (&["npm", "run", "build"], &["npm", "run", "build", "**"]),
            (
                &[r"C:\tools\cargo.exe", "build", "--release"],
                &["cargo", "build", "**"],
            ),
            (&["cargo", "build"], &["cargo", "build", "**"]),
            (
                &["deploytool", "sync", "./dist", "now"],
                &["deploytool", "sync", "**"],
            ),
            (&["deploytool", "--verbose", "sync"], &["deploytool", "**"]),
            (&["deploytool", "C:/x", "sync"], &["deploytool", "**"]),
        ];
        for (argv, pattern) in cases {
            let rule = derived(&process(argv, WORK), AllowRuleScope::Anywhere).expect("derived");
            let expected: Vec<String> = pattern.iter().map(|part| (*part).to_owned()).collect();
            assert_eq!(rule.pattern(), Some(expected), "{argv:?}");
        }
    }

    #[test]
    fn interpreters_and_script_strings_cannot_be_allowed() {
        let cases: [&[&str]; 7] = [
            &["powershell", "-Command", "Get-ChildItem"],
            &["pwsh", "-ec", "ZQBjAGgAbwA="],
            &["cmd", "/c", "dir"],
            &["bash", "-c", "ls"],
            &["python", "-c", "print(1)"],
            &["node", "--version"],
            &["wsl", "ls"],
        ];
        for argv in cases {
            assert_eq!(
                refusal(&process(argv, WORK), AllowRuleScope::Folder),
                SCRIPT_RULE,
                "{argv:?}"
            );
        }
    }

    #[test]
    fn a_source_bearing_plan_cannot_be_allowed() {
        let mut process = plan(&["deploytool", "build.cfg"], WORK);
        process.language = Some(CodeLanguage::Python);
        process.source = Some("print(1)".to_owned());
        let action = ReviewedAction::Process {
            tool: "run_command".to_owned(),
            process,
        };
        assert_eq!(refusal(&action, AllowRuleScope::Anywhere), SOURCE_RULE);
    }

    #[test]
    fn a_command_the_vendor_pack_asks_about_or_denies_cannot_be_allowed() {
        assert_eq!(
            refusal(
                &process(&["npx", "cowsay", "hi"], WORK),
                AllowRuleScope::Folder
            ),
            VENDOR_RULE
        );
        assert_eq!(
            refusal(
                &process(&["git", "push", "--force"], WORK),
                AllowRuleScope::Folder
            ),
            VENDOR_RULE
        );
        assert_eq!(
            refusal(
                &changing(r"C:\work\.git\config", FileChangeKind::Delete),
                AllowRuleScope::Folder
            ),
            VENDOR_RULE
        );
    }

    #[test]
    fn the_folder_scope_refuses_anything_outside_the_working_folder() {
        assert_eq!(
            refusal(
                &process(&["cargo", "build"], r"D:\other"),
                AllowRuleScope::Folder
            ),
            OUTSIDE_FOLDER
        );
        assert_eq!(
            refusal(
                &changing(r"D:\secret\a.txt", FileChangeKind::Modify),
                AllowRuleScope::Folder
            ),
            OUTSIDE_FOLDER
        );
        assert_eq!(
            refusal(
                &changing(r"C:\workshop\a.txt", FileChangeKind::Delete),
                AllowRuleScope::Folder
            ),
            OUTSIDE_FOLDER
        );
        let none = derive(
            &process(&["cargo", "build"], WORK),
            AllowRuleScope::Folder,
            None,
            &vendor(),
            NOW,
        )
        .expect_err("refused");
        assert_eq!(none.message, NO_WORKING_FOLDER);
        assert!(
            derived(
                &process(&["cargo", "build"], r"D:\other"),
                AllowRuleScope::Anywhere
            )
            .is_ok()
        );
    }

    #[test]
    fn creating_files_alone_and_a_plain_tool_give_nothing_to_allow() {
        assert_eq!(
            refusal(
                &changing(r"C:\work\a.txt", FileChangeKind::Create),
                AllowRuleScope::Folder
            ),
            NOTHING_TO_ALLOW
        );
        assert_eq!(
            refusal(&ReviewedAction::Plain, AllowRuleScope::Folder),
            NOTHING_TO_ALLOW
        );
    }

    #[test]
    fn a_path_with_pattern_characters_cannot_become_a_rule() {
        assert_eq!(
            refusal(
                &changing(r"C:\work\{draft}.txt", FileChangeKind::Delete),
                AllowRuleScope::Anywhere
            ),
            PATTERN_CHARACTERS
        );
    }

    #[test]
    fn a_folder_rule_allows_the_command_only_inside_the_folder() {
        let packs = tempfile::tempdir().expect("dir");
        let rule = derived(
            &process(&["cargo", "build", "--release"], WORK),
            AllowRuleScope::Folder,
        )
        .expect("derived");
        assert_eq!(
            rule.text,
            "Разрешено из подтверждения: `cargo build **` в папке C:/work"
        );
        assert_eq!(rule.created_at, Some(NOW));
        let written = allow(packs.path(), &KEY, rule).expect("written");
        assert!(written.rule.id.starts_with("user."));
        assert!(packs.path().join("user.json.mac").is_file());
        assert!(allowed(
            packs.path(),
            &process(&["cargo", "build"], "C:/work/crates")
        ));
        assert!(!allowed(
            packs.path(),
            &process(&["cargo", "build"], "C:/other")
        ));
        assert!(!allowed(packs.path(), &process(&["cargo", "test"], WORK)));
    }

    #[test]
    fn a_file_rule_covers_the_folder_or_only_the_exact_files() {
        let packs = tempfile::tempdir().expect("dir");
        let old = changing(r"C:\work\old.txt", FileChangeKind::Delete);
        let exact = derived(&old, AllowRuleScope::Anywhere).expect("derived");
        allow(packs.path(), &KEY, exact).expect("written");
        assert!(allowed(packs.path(), &old));
        assert!(!allowed(
            packs.path(),
            &changing("C:/work/new.txt", FileChangeKind::Delete)
        ));
        let folder = derived(&old, AllowRuleScope::Folder).expect("derived");
        allow(packs.path(), &KEY, folder).expect("written");
        assert!(allowed(
            packs.path(),
            &changing("C:/work/src/new.txt", FileChangeKind::Delete)
        ));
        assert!(!allowed(
            packs.path(),
            &changing("C:/workshop/a.txt", FileChangeKind::Delete)
        ));
    }

    #[test]
    fn the_same_rule_twice_is_stored_once() {
        let packs = tempfile::tempdir().expect("dir");
        let first = written(packs.path(), &["deploytool", "--a"]);
        let second = written(packs.path(), &["deploytool", "--b"]);
        assert_eq!(first, second);
        assert_eq!(list(packs.path(), Some(&KEY)).rules.len(), 1);
    }

    #[test]
    fn a_hand_edited_allow_is_ignored_and_flagged_until_confirmed() {
        let packs = tempfile::tempdir().expect("dir");
        written(packs.path(), &["cargo", "build"]);
        let file = packs.path().join(USER_PACK_FILE);
        let mut pack: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).expect("reads")).expect("parses");
        let rules = rules_of(&mut pack).expect("rules");
        rules.push(serde_json::json!({
            "id": "user.mine", "effect": "allow", "text": "Мои тесты",
            "process": { "argv": ["deploytool", "**"] }
        }));
        rules.push(serde_json::json!({
            "id": "user.no-dist", "effect": "deny", "text": "Нет",
            "files": { "paths": ["C:/work/dist/**"] }
        }));
        std::fs::write(
            &file,
            serde_json::to_string_pretty(&pack).expect("serialises"),
        )
        .expect("writes");

        assert!(!allowed(packs.path(), &process(&["deploytool"], WORK)));
        assert!(!allowed(packs.path(), &process(&["cargo", "build"], WORK)));
        assert!(matches!(
            evaluate(
                packs.path(),
                &changing("C:/work/dist/a.js", FileChangeKind::Delete)
            ),
            RuleOutcome::Deny { .. }
        ));
        let listed = list(packs.path(), Some(&KEY));
        assert_eq!(listed.unconfirmed, vec![USER_PACK_FILE.to_owned()]);
        assert!(
            listed
                .rules
                .iter()
                .filter(|rule| rule.effect == PolicyRuleEffect::Allow)
                .all(|rule| !rule.active)
        );
        assert!(
            listed
                .rules
                .iter()
                .any(|rule| rule.id == "user.no-dist" && rule.active)
        );

        confirm(packs.path(), &KEY, USER_PACK_FILE).expect("confirmed");
        assert!(allowed(packs.path(), &process(&["deploytool"], WORK)));
        assert!(unconfirmed(packs.path()).is_empty());
    }

    #[test]
    fn a_new_rule_sets_an_unconfirmed_pack_aside_instead_of_signing_it() {
        let packs = tempfile::tempdir().expect("dir");
        std::fs::write(
            packs.path().join(USER_PACK_FILE),
            r#"{ "id": "user", "allows": true, "rules": [ { "id": "user.planted", "effect": "allow", "text": "t", "process": { "argv": ["deploytool", "**"] } } ] }"#,
        )
        .expect("writes");
        let rule = derived(
            &process(&["cargo", "build"], WORK),
            AllowRuleScope::Anywhere,
        )
        .expect("derived");
        let allowed_rule = allow(packs.path(), &KEY, rule).expect("written");
        assert_eq!(
            allowed_rule.set_aside.as_deref(),
            Some("user.unconfirmed.json")
        );
        assert!(allowed(packs.path(), &process(&["cargo", "build"], WORK)));
        assert!(!allowed(packs.path(), &process(&["deploytool"], WORK)));
        assert_eq!(
            unconfirmed(packs.path()),
            vec!["user.unconfirmed.json".to_owned()]
        );
    }

    #[test]
    fn removing_a_rule_keeps_the_pack_signed_and_stops_the_allow() {
        let packs = tempfile::tempdir().expect("dir");
        let cargo = written(packs.path(), &["cargo", "build"]);
        written(packs.path(), &["deploytool", "sync"]);
        assert_eq!(
            remove(packs.path(), Some(&KEY), &cargo.id).expect("removed"),
            USER_PACK_FILE
        );
        assert!(!allowed(packs.path(), &process(&["cargo", "build"], WORK)));
        assert!(allowed(
            packs.path(),
            &process(&["deploytool", "sync"], WORK)
        ));
        let missing = remove(packs.path(), Some(&KEY), &cargo.id).expect_err("gone");
        assert_eq!(missing.message, RULE_MISSING);
    }

    #[test]
    fn removing_from_an_unconfirmed_pack_does_not_confirm_it() {
        let packs = tempfile::tempdir().expect("dir");
        std::fs::write(
            packs.path().join("mine.json"),
            r#"{ "id": "mine", "allows": true, "rules": [
                { "id": "mine.a", "effect": "allow", "text": "a", "process": { "argv": ["deploytool", "**"] } },
                { "id": "mine.b", "effect": "allow", "text": "b", "process": { "argv": ["othertool", "**"] } }
            ] }"#,
        )
        .expect("writes");
        remove(packs.path(), Some(&KEY), "mine.a").expect("removed");
        assert!(!allowed(packs.path(), &process(&["othertool"], WORK)));
        assert_eq!(unconfirmed(packs.path()), vec!["mine.json".to_owned()]);
    }

    #[test]
    fn the_cli_pack_is_migrated_once_and_waits_for_confirmation() {
        let packs = tempfile::tempdir().expect("dir");
        std::fs::write(
            packs.path().join(LEGACY_PACK_FILE),
            r#"{ "id": "user.cli", "allows": true, "rules": [ { "id": "user.cli.a", "effect": "allow", "text": "a", "process": { "argv": ["deploytool", "**"] } } ] }"#,
        )
        .expect("writes");
        assert_eq!(
            migrate(packs.path()).expect("migrated").as_deref(),
            Some(USER_PACK_FILE)
        );
        assert!(!packs.path().join(LEGACY_PACK_FILE).exists());
        assert_eq!(migrate(packs.path()).expect("nothing left"), None);
        assert_eq!(unconfirmed(packs.path()), vec![USER_PACK_FILE.to_owned()]);
        assert!(!allowed(packs.path(), &process(&["deploytool"], WORK)));
    }

    #[test]
    fn a_damaged_or_absent_pack_cannot_be_confirmed() {
        let packs = tempfile::tempdir().expect("dir");
        std::fs::write(packs.path().join("broken.json"), "{ not json").expect("writes");
        assert_eq!(
            confirm(packs.path(), &KEY, "broken.json")
                .expect_err("refused")
                .code,
            AppErrorCode::Conflict
        );
        assert_eq!(
            confirm(packs.path(), &KEY, "absent.json")
                .expect_err("refused")
                .message,
            PACK_MISSING
        );
    }
}
