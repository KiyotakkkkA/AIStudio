use std::path::{Path, PathBuf};
use std::time::SystemTime;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use zvs_contract::{CodeLanguage, ProcessPlanDto};

use super::action::{FileAction, FileOperation, ReviewedAction, UNREAD_SCRIPT};
use super::integrity::{MAC_EXTENSION, PackKey, signed};

pub const VENDOR_PACK: &str = include_str!("vendor.json");
pub const ANY_REST: &str = "**";
pub const ANY_PROGRAM: &str = "*";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleEffect {
    Allow,
    Review,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackOrigin {
    Vendor,
    User,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileRuleSpec {
    paths: Vec<String>,
    #[serde(default)]
    operations: Vec<FileOperation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProcessRuleSpec {
    #[serde(default)]
    argv: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    script_string: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodeRuleSpec {
    languages: Vec<CodeLanguage>,
    #[serde(default)]
    avoid: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuleSpec {
    id: String,
    effect: RuleEffect,
    text: String,
    #[serde(default)]
    files: Option<FileRuleSpec>,
    #[serde(default)]
    process: Option<ProcessRuleSpec>,
    #[serde(default)]
    code: Option<CodeRuleSpec>,
    #[serde(default, rename = "createdAt")]
    _created_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackSpec {
    id: String,
    allows: bool,
    rules: Vec<RuleSpec>,
}

#[derive(Debug, Clone)]
enum Matcher {
    Files {
        paths: Vec<Regex>,
        operations: Vec<FileOperation>,
    },
    Process {
        argv: Vec<ArgPattern>,
        cwd: Option<Regex>,
        script_string: Option<bool>,
    },
    Code {
        languages: Vec<CodeLanguage>,
        avoid: Vec<Regex>,
    },
}

#[derive(Debug, Clone)]
enum ArgPattern {
    Rest,
    One(Regex),
}

fn match_args(patterns: &[ArgPattern], args: &[&str]) -> bool {
    match patterns.split_first() {
        None => args.is_empty(),
        Some((ArgPattern::Rest, tail)) => {
            (0..=args.len()).any(|skip| match_args(tail, &args[skip..]))
        }
        Some((ArgPattern::One(pattern), tail)) => args
            .split_first()
            .is_some_and(|(head, rest)| pattern.is_match(head) && match_args(tail, rest)),
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub effect: RuleEffect,
    pub text: String,
    pub origin: PackOrigin,
    matcher: Matcher,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleText {
    pub id: String,
    pub effect: RuleEffect,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleOutcome {
    Allow { rule: String, text: String },
    Review { rule: String, text: String },
    Ask { rule: String, text: String },
    Deny { rule: String },
    Unmatched,
}

fn translate(pattern: &str, separator: bool) -> Option<Regex> {
    let normalised = pattern.replace('\\', "/");
    let chars: Vec<char> = normalised.chars().collect();
    let mut out = String::from("(?i)^");
    let mut index = 0;
    let mut braces = 0usize;
    while index < chars.len() {
        match chars[index] {
            '{' => {
                braces += 1;
                out.push_str("(?:");
            }
            '}' if braces > 0 => {
                braces -= 1;
                out.push(')');
            }
            ',' if braces > 0 => out.push('|'),
            '*' if chars.get(index + 1) == Some(&'*') => {
                index += 1;
                if chars.get(index + 1) == Some(&'/') {
                    index += 1;
                    out.push_str("(?:.*/)?");
                } else {
                    out.push_str(".*");
                }
            }
            '*' if separator => out.push_str("[^/]*"),
            '*' => out.push_str(".*"),
            '?' if separator => out.push_str("[^/]"),
            '?' => out.push('.'),
            other => out.push_str(&regex::escape(&other.to_string())),
        }
        index += 1;
    }
    if braces > 0 {
        return None;
    }
    out.push('$');
    Regex::new(&out).ok()
}

pub fn path_glob(pattern: &str) -> Option<Regex> {
    translate(pattern, true)
}

fn arg_glob(pattern: &str) -> Option<Regex> {
    translate(pattern, false)
}

fn normalised(path: &str) -> String {
    path.replace('\\', "/")
}

fn program_name(program: &str) -> String {
    let name = normalised(program);
    let name = name.rsplit('/').next().unwrap_or_default().to_owned();
    let lower = name.to_lowercase();
    for extension in [".exe", ".cmd", ".bat", ".com"] {
        if let Some(stem) = lower.strip_suffix(extension) {
            return stem.to_owned();
        }
    }
    lower
}

impl Rule {
    fn compile(spec: RuleSpec, origin: PackOrigin) -> Result<Self, String> {
        if spec.id.trim().is_empty() || spec.text.trim().is_empty() {
            return Err("a rule needs an id and a text".to_owned());
        }
        let invalid = |pattern: &str| format!("rule {}: invalid pattern {pattern}", spec.id);
        let matcher = match (spec.files, spec.process, spec.code) {
            (None, None, Some(code)) => {
                if code.languages.is_empty() {
                    return Err(format!("rule {}: no languages", spec.id));
                }
                let avoid = code
                    .avoid
                    .iter()
                    .map(|pattern| {
                        RegexBuilder::new(pattern)
                            .case_insensitive(true)
                            .build()
                            .map_err(|_| invalid(pattern))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Matcher::Code {
                    languages: code.languages,
                    avoid,
                }
            }
            (Some(files), None, None) => {
                if files.paths.is_empty() {
                    return Err(format!("rule {}: no paths", spec.id));
                }
                let paths = files
                    .paths
                    .iter()
                    .map(|pattern| path_glob(pattern).ok_or_else(|| invalid(pattern)))
                    .collect::<Result<Vec<_>, _>>()?;
                Matcher::Files {
                    paths,
                    operations: files.operations,
                }
            }
            (None, Some(process), None) => {
                let program = process.argv.first();
                if program.is_some_and(|program| program == ANY_REST)
                    || (program.is_none() && process.script_string.is_none())
                {
                    return Err(format!("rule {}: argv needs a program", spec.id));
                }
                if spec.effect == RuleEffect::Allow
                    && program.is_some_and(|program| program == ANY_PROGRAM)
                    && process.argv[1..].iter().all(|pattern| pattern == ANY_REST)
                {
                    return Err(format!(
                        "rule {}: an allow rule must name a program or arguments",
                        spec.id
                    ));
                }
                let argv = process
                    .argv
                    .iter()
                    .map(|pattern| {
                        if pattern == ANY_REST {
                            Ok(ArgPattern::Rest)
                        } else {
                            arg_glob(pattern)
                                .map(ArgPattern::One)
                                .ok_or_else(|| invalid(pattern))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let cwd = process
                    .cwd
                    .as_deref()
                    .map(|pattern| path_glob(pattern).ok_or_else(|| invalid(pattern)))
                    .transpose()?;
                Matcher::Process {
                    argv,
                    cwd,
                    script_string: process.script_string,
                }
            }
            _ => {
                return Err(format!(
                    "rule {}: exactly one of files, process or code is required",
                    spec.id
                ));
            }
        };
        Ok(Self {
            id: spec.id,
            effect: spec.effect,
            text: spec.text,
            origin,
            matcher,
        })
    }

    fn matches_file(&self, action: &FileAction) -> bool {
        let Matcher::Files { paths, operations } = &self.matcher else {
            return false;
        };
        let matches = |path: &str| {
            let path = normalised(path);
            paths.iter().any(|pattern| pattern.is_match(&path))
        };
        (operations.is_empty() || operations.contains(&action.operation))
            && (matches(&action.path) || action.from.as_deref().is_some_and(matches))
    }

    fn matches_process(&self, process: &ProcessPlanDto) -> bool {
        let Matcher::Process {
            argv: patterns,
            cwd: folder,
            script_string,
        } = &self.matcher
        else {
            return false;
        };
        if script_string.is_some_and(|wanted| wanted != process.script_string) {
            return false;
        }
        let matched = patterns.is_empty()
            || process.argv.split_first().is_some_and(|(program, rest)| {
                let program = program_name(program);
                let arguments: Vec<&str> = std::iter::once(program.as_str())
                    .chain(rest.iter().map(String::as_str))
                    .collect();
                match_args(patterns, &arguments)
            });
        matched
            && folder
                .as_ref()
                .is_none_or(|folder| folder.is_match(&normalised(&process.cwd)))
    }

    fn matches_code(&self, language: CodeLanguage, source: &str) -> bool {
        let Matcher::Code { languages, avoid } = &self.matcher else {
            return false;
        };
        languages.contains(&language) && !avoid.iter().any(|pattern| pattern.is_match(source))
    }

    const fn is_files(&self) -> bool {
        matches!(self.matcher, Matcher::Files { .. })
    }

    const fn pinned(&self) -> bool {
        matches!(
            self.matcher,
            Matcher::Process {
                script_string: Some(true),
                ..
            }
        ) && !matches!(self.effect, RuleEffect::Allow)
    }

    pub fn text(&self) -> RuleText {
        RuleText {
            id: self.id.clone(),
            effect: self.effect,
            text: self.text.clone(),
        }
    }
}

pub fn compile_pack(text: &str, origin: PackOrigin) -> Result<Vec<Rule>, String> {
    let pack: PackSpec = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if pack.id.trim().is_empty() {
        return Err("a pack needs an id".to_owned());
    }
    let allows = pack
        .rules
        .iter()
        .any(|rule| rule.effect == RuleEffect::Allow);
    if allows && !pack.allows {
        return Err(format!(
            "pack {} contains an allow rule but declares allows: false",
            pack.id
        ));
    }
    pack.rules
        .into_iter()
        .map(|rule| Rule::compile(rule, origin))
        .collect()
}

fn strongest<'a>(rules: impl Iterator<Item = &'a Rule>) -> Option<&'a Rule> {
    rules.min_by_key(|rule| std::cmp::Reverse(rule.effect))
}

fn resolve<'a>(matched: &[&'a Rule]) -> Option<&'a Rule> {
    let vendor = || {
        matched
            .iter()
            .copied()
            .filter(|rule| rule.origin == PackOrigin::Vendor)
    };
    vendor()
        .find(|rule| rule.effect == RuleEffect::Deny)
        .or_else(|| strongest(vendor().filter(|rule| rule.pinned())))
        .or_else(|| {
            strongest(
                matched
                    .iter()
                    .copied()
                    .filter(|rule| rule.origin == PackOrigin::User),
            )
        })
        .or_else(|| strongest(vendor()))
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<Rule>,
    unconfirmed: Vec<String>,
}

pub fn user_pack(
    text: &str,
    file: &Path,
    key: Option<&PackKey>,
) -> Result<(Vec<Rule>, bool), String> {
    let mut rules = compile_pack(text, PackOrigin::User)?;
    if signed(file, text.as_bytes(), key) {
        return Ok((rules, false));
    }
    let before = rules.len();
    rules.retain(|rule| rule.effect != RuleEffect::Allow);
    let ignored = rules.len() != before;
    Ok((rules, ignored))
}

impl RuleSet {
    pub const fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            unconfirmed: Vec::new(),
        }
    }

    pub fn unconfirmed(&self) -> &[String] {
        &self.unconfirmed
    }

    pub fn load(vendor: &str, user: Option<&Path>, key: Option<&PackKey>) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let mut rules = match compile_pack(vendor, PackOrigin::Vendor) {
            Ok(rules) => rules,
            Err(problem) => {
                problems.push(format!("vendor: {problem}"));
                Vec::new()
            }
        };
        let mut unconfirmed = Vec::new();
        for file in pack_files(user) {
            let name = file.to_string_lossy().into_owned();
            match std::fs::read_to_string(&file)
                .map_err(|error| error.to_string())
                .and_then(|text| user_pack(&text, &file, key))
            {
                Ok((pack, ignored)) => {
                    rules.extend(pack);
                    if ignored {
                        unconfirmed.push(pack_name(&file));
                    }
                }
                Err(problem) => problems.push(format!("{name}: {problem}")),
            }
        }
        (Self { rules, unconfirmed }, problems)
    }

    pub fn texts(&self, files: bool) -> Vec<RuleText> {
        self.rules
            .iter()
            .filter(|rule| rule.is_files() == files)
            .map(Rule::text)
            .collect()
    }

    fn resolve_items<'a>(&'a self, items: Vec<Vec<&'a Rule>>) -> RuleOutcome {
        let decided: Vec<Option<&Rule>> = items.iter().map(|matched| resolve(matched)).collect();
        if let Some(rule) = decided
            .iter()
            .flatten()
            .find(|rule| rule.effect == RuleEffect::Deny)
        {
            return RuleOutcome::Deny {
                rule: rule.id.clone(),
            };
        }
        if let Some(rule) = decided
            .iter()
            .flatten()
            .find(|rule| rule.effect == RuleEffect::Ask)
        {
            return RuleOutcome::Ask {
                rule: rule.id.clone(),
                text: rule.text.clone(),
            };
        }
        if let Some(rule) = decided
            .iter()
            .flatten()
            .find(|rule| rule.effect == RuleEffect::Review)
        {
            return RuleOutcome::Review {
                rule: rule.id.clone(),
                text: rule.text.clone(),
            };
        }
        match decided.first() {
            Some(Some(first)) if decided.iter().all(Option::is_some) => RuleOutcome::Allow {
                rule: first.id.clone(),
                text: first.text.clone(),
            },
            _ => RuleOutcome::Unmatched,
        }
    }

    pub fn evaluate(&self, action: &ReviewedAction) -> RuleOutcome {
        match action {
            ReviewedAction::Files { actions, .. } => {
                let items: Vec<Vec<&Rule>> = actions
                    .iter()
                    .map(|action| (action, self.file_rules(action)))
                    .filter(|(action, matched)| {
                        action.operation.destructive() || !matched.is_empty()
                    })
                    .map(|(_, matched)| matched)
                    .collect();
                if items.is_empty() {
                    return RuleOutcome::Unmatched;
                }
                self.resolve_items(items)
            }
            ReviewedAction::Process { process, .. }
                if process.language.is_some() && process.source.is_some() =>
            {
                let language = process.language.unwrap_or(CodeLanguage::Python);
                let source = process.source.as_deref().unwrap_or_default();
                let code: Vec<&Rule> = if source == UNREAD_SCRIPT {
                    Vec::new()
                } else {
                    self.rules
                        .iter()
                        .filter(|rule| rule.matches_code(language, source))
                        .collect()
                };
                let mut items = vec![code];
                let command = self.process_rules(process);
                if !command.is_empty() {
                    items.push(command);
                }
                self.resolve_items(items)
            }
            ReviewedAction::Process { process, .. }
                if process.language.is_none() && !process.argv.is_empty() =>
            {
                self.resolve_items(vec![self.process_rules(process)])
            }
            _ => RuleOutcome::Unmatched,
        }
    }

    fn file_rules(&self, action: &FileAction) -> Vec<&Rule> {
        self.rules
            .iter()
            .filter(|rule| rule.matches_file(action))
            .collect()
    }

    fn process_rules(&self, process: &ProcessPlanDto) -> Vec<&Rule> {
        self.rules
            .iter()
            .filter(|rule| rule.matches_process(process))
            .collect()
    }

    pub fn denied(&self, action: &ReviewedAction) -> Option<String> {
        let deny = |rule: &&Rule| rule.effect == RuleEffect::Deny;
        let found = match action {
            ReviewedAction::Files { actions, .. } => actions
                .iter()
                .find_map(|action| self.file_rules(action).into_iter().find(deny)),
            ReviewedAction::Process { process, .. } => {
                let code = match (process.language, process.source.as_deref()) {
                    (Some(language), Some(source)) if source != UNREAD_SCRIPT => self
                        .rules
                        .iter()
                        .filter(|rule| rule.matches_code(language, source))
                        .find(deny),
                    _ => None,
                };
                code.or_else(|| self.process_rules(process).into_iter().find(deny))
            }
            _ => None,
        };
        found.map(|rule| rule.id.clone())
    }
}

pub fn pack_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn files_with(folder: Option<&Path>, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = folder
        .and_then(|folder| std::fs::read_dir(folder).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            })
        })
        .collect();
    files.sort();
    files
}

pub fn pack_files(folder: Option<&Path>) -> Vec<PathBuf> {
    files_with(folder, &["json"])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackStamp(Vec<(PathBuf, Option<SystemTime>, u64)>);

impl PackStamp {
    pub fn read(folder: Option<&Path>) -> Self {
        Self(
            files_with(folder, &["json", MAC_EXTENSION])
                .into_iter()
                .map(|file| {
                    let metadata = std::fs::metadata(&file).ok();
                    let modified = metadata
                        .as_ref()
                        .and_then(|metadata| metadata.modified().ok());
                    let length = metadata.map_or(0, |metadata| metadata.len());
                    (file, modified, length)
                })
                .collect(),
        )
    }
}
