use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use regex::{Regex, RegexBuilder};

static CACHE: LazyLock<Mutex<HashMap<String, Option<Regex>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn to_regex(pattern: &str) -> Option<Regex> {
    let characters: Vec<char> = pattern.chars().collect();
    let mut source = String::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '*' {
            if characters.get(index + 1) == Some(&'*') {
                index += 1;
                if characters.get(index + 1) == Some(&'/') {
                    index += 1;
                }
                source.push_str("(?:[^/]*(?:/|$))*");
            } else {
                source.push_str("[^/]*");
            }
        } else if character == '?' {
            source.push_str("[^/]");
        } else {
            source.push_str(&regex::escape(&character.to_string()));
        }
        index += 1;
    }
    RegexBuilder::new(&format!("^{source}$"))
        .case_insensitive(true)
        .build()
        .ok()
}

pub fn normalize_relative(path: &str) -> String {
    let forward = path.replace('\\', "/");
    forward
        .strip_prefix("./")
        .or_else(|| forward.strip_prefix('/'))
        .unwrap_or(&forward)
        .to_owned()
}

pub fn matches_pattern(relative_path: &str, pattern: &str) -> bool {
    let key = pattern.replace('\\', "/");
    let expression = {
        let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(key.clone())
            .or_insert_with(|| {
                if key.contains('/') {
                    to_regex(&key)
                } else {
                    to_regex(&format!("**/{key}"))
                }
            })
            .clone()
    };
    expression.is_some_and(|expression| expression.is_match(&normalize_relative(relative_path)))
}

static ANCHORED: LazyLock<Mutex<HashMap<String, Option<Regex>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn matches_anchored(relative_path: &str, pattern: &str) -> bool {
    let key = pattern.replace('\\', "/");
    let expression = {
        let mut cache = ANCHORED.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(key.clone())
            .or_insert_with(|| to_regex(&key))
            .clone()
    };
    expression.is_some_and(|expression| expression.is_match(&normalize_relative(relative_path)))
}

pub fn excludes(relative_path: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|pattern| {
        matches_pattern(relative_path, pattern)
            || matches_pattern(relative_path, &format!("{pattern}/**"))
    })
}

pub fn accepts(relative_path: &str, include: &[String], exclude: &[String]) -> bool {
    if excludes(relative_path, exclude) {
        return false;
    }
    include.is_empty()
        || include
            .iter()
            .any(|pattern| matches_pattern(relative_path, pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn patterns_match_by_segment_and_treat_a_bare_name_as_any_depth() {
        assert!(accepts("docs/guide.md", &patterns(&["**/*.md"]), &[]));
        assert!(!accepts("docs/guide.txt", &patterns(&["**/*.md"]), &[]));
        assert!(accepts("guide.md", &patterns(&["*.md"]), &[]));
        assert!(!accepts(
            "node_modules/x/y.md",
            &[],
            &patterns(&["node_modules"])
        ));
        assert!(accepts("src/a.md", &[], &[]));
    }

    #[test]
    fn a_star_stays_inside_a_segment_and_a_question_mark_is_one_character() {
        assert!(matches_pattern("docs/a.md", "docs/*.md"));
        assert!(!matches_pattern("docs/deep/a.md", "docs/*.md"));
        assert!(matches_pattern("docs/deep/a.md", "docs/**/*.md"));
        assert!(matches_pattern("docs/a.md", "docs/**/*.md"));
        assert!(matches_pattern("a1.txt", "a?.txt"));
        assert!(!matches_pattern("a12.txt", "a?.txt"));
        assert!(!matches_pattern("a/.txt", "a?.txt"));
    }

    #[test]
    fn matching_ignores_case_and_backslashes_and_a_leading_dot_slash() {
        assert!(matches_pattern(r"Docs\Guide.MD", "docs/*.md"));
        assert!(matches_pattern("./docs/guide.md", "docs/*.md"));
        assert!(matches_pattern("/docs/guide.md", "docs/*.md"));
        assert!(matches_pattern("ОТЧЁТ.md", "отчёт.md"));
    }

    #[test]
    fn regex_characters_in_a_pattern_are_literal() {
        assert!(matches_pattern("notes (1).md", "notes (1).md"));
        assert!(!matches_pattern("notesx(1).md", "notes.(1).md"));
        assert!(matches_pattern("a+b[c].md", "a+b[c].md"));
        assert!(matches_pattern("price$^.md", "price$^.md"));
    }

    #[test]
    fn an_excluded_folder_matches_at_any_depth_and_its_own_contents_from_the_root() {
        let exclude = patterns(&["build", "docs/private"]);
        assert!(excludes("build", &exclude));
        assert!(excludes("src/build", &exclude));
        assert!(!excludes("src/build/out.js", &exclude));
        assert!(excludes("docs/private/a.md", &exclude));
        assert!(!excludes("docs/public/a.md", &exclude));
        assert!(!excludes("builder/a.md", &exclude));
    }

    #[test]
    fn include_is_checked_only_after_exclude() {
        let include = patterns(&["**/*.md"]);
        let exclude = patterns(&["drafts"]);
        assert!(accepts("notes/a.md", &include, &exclude));
        assert!(!accepts("drafts/a.md", &include, &exclude));
        assert!(!accepts("notes/a.txt", &include, &exclude));
    }

    #[test]
    fn normalising_strips_one_leading_marker() {
        assert_eq!(normalize_relative(r".\a\b"), "a/b");
        assert_eq!(normalize_relative("/a"), "a");
        assert_eq!(normalize_relative("a/b"), "a/b");
    }
}
