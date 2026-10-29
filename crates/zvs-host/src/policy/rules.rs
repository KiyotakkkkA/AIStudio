use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::action::{FileAction, FileOperation, ReviewedAction};

pub const VENDOR_PACK: &str = include_str!("vendor.json");
pub const ANY_REST: &str = "**";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleEffect {
    Allow,
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
    argv: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
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
        let matcher = match (spec.files, spec.process) {
            (Some(files), None) => {
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
            (None, Some(process)) => {
                if process
                    .argv
                    .first()
                    .is_none_or(|program| program == ANY_REST)
                {
                    return Err(format!("rule {}: argv needs a program", spec.id));
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
                Matcher::Process { argv, cwd }
            }
            _ => {
                return Err(format!(
                    "rule {}: exactly one of files or process is required",
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
        let path = normalised(&action.path);
        (operations.is_empty() || operations.contains(&action.operation))
            && paths.iter().any(|pattern| pattern.is_match(&path))
    }

    fn matches_process(&self, argv: &[String], cwd: &str) -> bool {
        let Matcher::Process {
            argv: patterns,
            cwd: folder,
        } = &self.matcher
        else {
            return false;
        };
        let Some((program, rest)) = argv.split_first() else {
            return false;
        };
        let program = program_name(program);
        let arguments: Vec<&str> = std::iter::once(program.as_str())
            .chain(rest.iter().map(String::as_str))
            .collect();
        match_args(patterns, &arguments)
            && folder
                .as_ref()
                .is_none_or(|folder| folder.is_match(&normalised(cwd)))
    }

    const fn is_files(&self) -> bool {
        matches!(self.matcher, Matcher::Files { .. })
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
    rules.max_by_key(|rule| rule.effect)
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
}

impl RuleSet {
    pub const fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    pub fn load(vendor: &str, user: Option<&Path>) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let mut rules = match compile_pack(vendor, PackOrigin::Vendor) {
            Ok(rules) => rules,
            Err(problem) => {
                problems.push(format!("vendor: {problem}"));
                Vec::new()
            }
        };
        let mut files: Vec<_> = user
            .and_then(|folder| std::fs::read_dir(folder).ok())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            })
            .collect();
        files.sort();
        for file in files {
            let name = file.to_string_lossy().into_owned();
            match std::fs::read_to_string(&file)
                .map_err(|error| error.to_string())
                .and_then(|text| compile_pack(&text, PackOrigin::User))
            {
                Ok(pack) => rules.extend(pack),
                Err(problem) => problems.push(format!("{name}: {problem}")),
            }
        }
        (Self { rules }, problems)
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
                    .filter(|action| action.operation.destructive())
                    .map(|action| {
                        self.rules
                            .iter()
                            .filter(|rule| rule.matches_file(action))
                            .collect()
                    })
                    .collect();
                if items.is_empty() {
                    return RuleOutcome::Unmatched;
                }
                self.resolve_items(items)
            }
            ReviewedAction::Process { process, .. }
                if process.language.is_none() && !process.argv.is_empty() =>
            {
                let matched = self
                    .rules
                    .iter()
                    .filter(|rule| rule.matches_process(&process.argv, &process.cwd))
                    .collect();
                self.resolve_items(vec![matched])
            }
            _ => RuleOutcome::Unmatched,
        }
    }
}
