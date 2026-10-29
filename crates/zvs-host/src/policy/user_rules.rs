use std::path::Path;

use serde::{Deserialize, Serialize};
use zvs_contract::{AllowRuleChangeDto, AllowRuleInput, AppError, AppErrorCode, FileChangeKind};

use super::action::FileOperation;
use super::rules::{ANY_REST, PackOrigin, RuleEffect, compile_pack};
use crate::platform::ids::new_id;

pub const CLI_PACK_FILE: &str = "user.cli.json";
pub const CLI_PACK_ID: &str = "user.cli";
pub const NOTHING_TO_ALLOW: &str = "В запросе нет действия, для которого создаётся правило";
pub const OUTSIDE_FOLDER: &str = "Действие выходит за пределы рабочей папки";
pub const PATTERN_CHARACTERS: &str = "Путь содержит символы, которые правило не может выразить";
pub const PACK_DAMAGED: &str = "Файл правил повреждён; исправьте или удалите его";
pub const PACK_NOT_SAVED: &str = "Не удалось сохранить правило";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRule {
    id: String,
    effect: RuleEffect,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    files: Option<FilesSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    process: Option<ProcessSpec>,
}

impl StoredRule {
    fn same_matcher(&self, other: &Self) -> bool {
        self.files == other.files && self.process == other.process
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
            id: CLI_PACK_ID.to_owned(),
            allows: true,
            rules: Vec::new(),
        }
    }
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
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            PATTERN_CHARACTERS,
        ));
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

fn storage(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, PACK_NOT_SAVED).with_detail("reason", error.to_string())
}

fn outside() -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, OUTSIDE_FOLDER)
}

const fn operation(kind: FileChangeKind) -> Option<FileOperation> {
    match kind {
        FileChangeKind::Create => None,
        FileChangeKind::Modify => Some(FileOperation::Overwrite),
        FileChangeKind::Move => Some(FileOperation::MoveOnto),
        FileChangeKind::Delete => Some(FileOperation::Delete),
    }
}

const fn operation_name(operation: FileOperation) -> &'static str {
    match operation {
        FileOperation::Create => "create",
        FileOperation::Overwrite => "overwrite",
        FileOperation::Move => "move",
        FileOperation::MoveOnto => "move onto",
        FileOperation::Delete => "delete",
    }
}

fn files_rule(changes: &[AllowRuleChangeDto], folder: Option<&str>) -> Result<FilesSpec, AppError> {
    let destructive: Vec<(&AllowRuleChangeDto, FileOperation)> = changes
        .iter()
        .filter_map(|change| operation(change.kind).map(|operation| (change, operation)))
        .collect();
    if destructive.is_empty() {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            NOTHING_TO_ALLOW,
        ));
    }
    let mut operations: Vec<FileOperation> = Vec::new();
    for (_, operation) in &destructive {
        if !operations.contains(operation) {
            operations.push(*operation);
        }
    }
    let paths = match folder {
        Some(folder) => {
            let folder = literal(folder)?;
            if destructive
                .iter()
                .any(|(change, _)| !inside(&folder, &normalised(&change.path)))
            {
                return Err(outside());
            }
            vec![format!("{folder}/**")]
        }
        None => {
            let mut paths: Vec<String> = Vec::new();
            for (change, _) in &destructive {
                let path = literal(&change.path)?;
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
        && chars.all(|next| next.is_ascii_alphanumeric() || matches!(next, '.' | '_' | ':' | '-'))
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

fn process_rule(argv: &[String], cwd: &str, folder: Option<&str>) -> Result<ProcessSpec, AppError> {
    let Some((program, rest)) = argv.split_first() else {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            NOTHING_TO_ALLOW,
        ));
    };
    let mut pattern = vec![literal(&program_name(program))?];
    if let Some(command) = rest.first().filter(|argument| word(argument)) {
        pattern.push(command.clone());
    }
    pattern.push(ANY_REST.to_owned());
    let cwd = match folder {
        Some(folder) => {
            let folder = literal(folder)?;
            if !inside(&folder, &normalised(cwd)) {
                return Err(outside());
            }
            Some(format!("{{{folder},{folder}/**}}"))
        }
        None => None,
    };
    Ok(ProcessSpec { argv: pattern, cwd })
}

fn describe(rule: &StoredRule, folder: Option<&str>) -> String {
    let place = folder.map_or_else(
        || "in any folder".to_owned(),
        |folder| format!("in {}", normalised(folder)),
    );
    if let Some(files) = &rule.files {
        let operations: Vec<&str> = files
            .operations
            .iter()
            .map(|operation| operation_name(*operation))
            .collect();
        return format!(
            "Allowed from the CLI: {} of {}",
            operations.join(", "),
            files.paths.join(", ")
        );
    }
    let argv = rule
        .process
        .as_ref()
        .map(|process| process.argv.join(" "))
        .unwrap_or_default();
    format!("Allowed from the CLI: `{argv}` {place}")
}

fn load(file: &Path) -> Result<StoredPack, AppError> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StoredPack::default());
        }
        Err(error) => return Err(storage(&error)),
    };
    let damaged = || AppError::new(AppErrorCode::Conflict, PACK_DAMAGED);
    compile_pack(&text, PackOrigin::User).map_err(|_| damaged())?;
    serde_json::from_str(&text).map_err(|_| damaged())
}

fn save(file: &Path, pack: &StoredPack) -> Result<(), AppError> {
    let failed = |reason: String| {
        AppError::new(AppErrorCode::Unknown, PACK_NOT_SAVED).with_detail("reason", reason)
    };
    let text = serde_json::to_string_pretty(pack).map_err(|error| failed(error.to_string()))?;
    compile_pack(&text, PackOrigin::User).map_err(failed)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| storage(&error))?;
    }
    let staged = file.with_extension("json.tmp");
    std::fs::write(&staged, format!("{text}\n")).map_err(|error| storage(&error))?;
    std::fs::rename(&staged, file).map_err(|error| storage(&error))
}

pub fn allow(folder: &Path, input: &AllowRuleInput) -> Result<WrittenRule, AppError> {
    let mut rule = StoredRule {
        id: format!("{CLI_PACK_ID}.{}", new_id().simple()),
        effect: RuleEffect::Allow,
        text: String::new(),
        files: None,
        process: None,
    };
    match input {
        AllowRuleInput::Files { changes, folder } => {
            rule.files = Some(files_rule(changes, folder.as_deref())?);
        }
        AllowRuleInput::Process { argv, cwd, folder } => {
            rule.process = Some(process_rule(argv, cwd, folder.as_deref())?);
        }
    }
    rule.text = describe(&rule, input.folder());
    let file = folder.join(CLI_PACK_FILE);
    let mut pack = load(&file)?;
    if let Some(existing) = pack
        .rules
        .iter()
        .find(|known| known.effect == RuleEffect::Allow && known.same_matcher(&rule))
    {
        return Ok(WrittenRule {
            id: existing.id.clone(),
            text: existing.text.clone(),
        });
    }
    let written = WrittenRule {
        id: rule.id.clone(),
        text: rule.text.clone(),
    };
    pack.rules.push(rule);
    save(&file, &pack)?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use zvs_contract::{FileDiffDto, ProcessPlanDto};

    use super::super::action::ReviewedAction;
    use super::super::rules::{RuleOutcome, RuleSet, VENDOR_PACK};
    use super::*;

    const WORK: &str = "C:/work";

    fn process(argv: &[&str], cwd: &str) -> ReviewedAction {
        ReviewedAction::Process {
            tool: "run_command".to_owned(),
            process: ProcessPlanDto {
                program: argv[0].to_owned(),
                language: None,
                argv: argv.iter().map(|part| (*part).to_owned()).collect(),
                cwd: cwd.to_owned(),
                source: None,
                packages: Vec::new(),
            },
        }
    }

    fn deleting(path: &str) -> ReviewedAction {
        ReviewedAction::files(
            vec![FileDiffDto {
                path: path.to_owned(),
                from: None,
                kind: FileChangeKind::Delete,
                added: 0,
                removed: 1,
                diff: String::new(),
                truncated: false,
            }],
            |_| true,
        )
    }

    fn change(path: &str, kind: FileChangeKind) -> AllowRuleChangeDto {
        AllowRuleChangeDto {
            path: path.to_owned(),
            kind,
        }
    }

    fn command(argv: &[&str], folder: Option<&str>) -> AllowRuleInput {
        AllowRuleInput::Process {
            argv: argv.iter().map(|part| (*part).to_owned()).collect(),
            cwd: WORK.to_owned(),
            folder: folder.map(str::to_owned),
        }
    }

    fn evaluate(packs: &Path, action: &ReviewedAction) -> RuleOutcome {
        let (rules, problems) = RuleSet::load(VENDOR_PACK, Some(packs));
        assert!(problems.is_empty(), "{problems:?}");
        rules.evaluate(action)
    }

    fn allowed(outcome: &RuleOutcome) -> bool {
        matches!(outcome, RuleOutcome::Allow { .. })
    }

    #[test]
    fn a_workspace_command_rule_allows_the_subcommand_only_inside_the_folder() {
        let packs = tempfile::tempdir().expect("dir");
        let rule = allow(
            packs.path(),
            &command(
                &[r"C:\tools\deploytool.exe", "sync", "--all"],
                Some(r"C:\work"),
            ),
        )
        .expect("written");
        assert!(rule.id.starts_with("user.cli."));
        assert_eq!(
            rule.text,
            "Allowed from the CLI: `deploytool sync **` in C:/work"
        );

        assert!(allowed(&evaluate(
            packs.path(),
            &process(&["deploytool", "sync", "--dry-run"], WORK)
        )));
        assert!(allowed(&evaluate(
            packs.path(),
            &process(&["deploytool", "sync"], "C:/work/crates")
        )));
        assert!(!allowed(&evaluate(
            packs.path(),
            &process(&["deploytool", "sync"], "C:/other")
        )));
        assert!(!allowed(&evaluate(
            packs.path(),
            &process(&["deploytool", "push"], WORK)
        )));
    }

    #[test]
    fn a_global_command_rule_ignores_the_folder_and_skips_a_flag_as_subcommand() {
        let packs = tempfile::tempdir().expect("dir");
        allow(packs.path(), &command(&["git", "--no-pager", "log"], None)).expect("written");
        assert!(allowed(&evaluate(
            packs.path(),
            &process(&["git", "status"], "D:/elsewhere")
        )));
    }

    #[test]
    fn a_file_rule_covers_the_workspace_or_only_the_exact_files() {
        let packs = tempfile::tempdir().expect("dir");
        allow(
            packs.path(),
            &AllowRuleInput::Files {
                changes: vec![change(r"C:\work\old.txt", FileChangeKind::Delete)],
                folder: None,
            },
        )
        .expect("written");
        assert!(allowed(&evaluate(
            packs.path(),
            &deleting("C:/work/old.txt")
        )));
        assert!(!allowed(&evaluate(
            packs.path(),
            &deleting("C:/work/new.txt")
        )));

        allow(
            packs.path(),
            &AllowRuleInput::Files {
                changes: vec![change(r"C:\work\old.txt", FileChangeKind::Delete)],
                folder: Some(r"C:\work".to_owned()),
            },
        )
        .expect("written");
        assert!(allowed(&evaluate(
            packs.path(),
            &deleting("C:/work/src/new.txt")
        )));
        assert!(!allowed(&evaluate(
            packs.path(),
            &deleting("C:/workshop/a.txt")
        )));
    }

    #[test]
    fn a_workspace_rule_refuses_an_action_outside_the_folder() {
        let packs = tempfile::tempdir().expect("dir");
        let refused = allow(
            packs.path(),
            &AllowRuleInput::Files {
                changes: vec![change(r"D:\secret\a.txt", FileChangeKind::Modify)],
                folder: Some(r"C:\work".to_owned()),
            },
        )
        .expect_err("refused");
        assert_eq!(refused.message, OUTSIDE_FOLDER);
        assert!(!packs.path().join(CLI_PACK_FILE).exists());
    }

    #[test]
    fn creating_files_alone_gives_nothing_to_allow() {
        let packs = tempfile::tempdir().expect("dir");
        let refused = allow(
            packs.path(),
            &AllowRuleInput::Files {
                changes: vec![change(r"C:\work\a.txt", FileChangeKind::Create)],
                folder: None,
            },
        )
        .expect_err("refused");
        assert_eq!(refused.message, NOTHING_TO_ALLOW);
    }

    #[test]
    fn the_same_rule_twice_is_stored_once() {
        let packs = tempfile::tempdir().expect("dir");
        let first = allow(packs.path(), &command(&["npm", "run", "build"], None)).expect("first");
        let second = allow(packs.path(), &command(&["npm", "run", "lint"], None)).expect("second");
        assert_eq!(first, second);
        let pack: StoredPack = serde_json::from_str(
            &std::fs::read_to_string(packs.path().join(CLI_PACK_FILE)).expect("reads"),
        )
        .expect("parses");
        assert_eq!(pack.rules.len(), 1);
    }

    #[test]
    fn a_damaged_pack_is_left_alone() {
        let packs = tempfile::tempdir().expect("dir");
        let file = packs.path().join(CLI_PACK_FILE);
        std::fs::write(&file, "{ not json").expect("writes");
        let refused = allow(packs.path(), &command(&["npm", "test"], None)).expect_err("refused");
        assert_eq!(refused.code, AppErrorCode::Conflict);
        assert_eq!(std::fs::read_to_string(&file).expect("reads"), "{ not json");
    }

    #[test]
    fn a_path_with_pattern_characters_cannot_become_a_rule() {
        let packs = tempfile::tempdir().expect("dir");
        let refused = allow(
            packs.path(),
            &AllowRuleInput::Files {
                changes: vec![change(r"C:\work\{draft}.txt", FileChangeKind::Delete)],
                folder: None,
            },
        )
        .expect_err("refused");
        assert_eq!(refused.message, PATTERN_CHARACTERS);
    }
}
