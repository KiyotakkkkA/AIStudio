use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

pub const PURPOSE_MAX_CHARS: usize = 120;

const HEAD_BYTES: u64 = 8 * 1024;

static DESCRIPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"description\s*[=:]\s*(?:"([^"\n]+)"|'([^'\n]+)')"#).expect("a valid pattern")
});

fn head(file: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(file)
        .ok()?
        .take(HEAD_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(
        text.strip_prefix('\u{feff}')
            .map(str::to_owned)
            .unwrap_or(text),
    )
}

fn clean(line: &str) -> Option<String> {
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    let mut chars = line.chars();
    let kept: String = chars.by_ref().take(PURPOSE_MAX_CHARS).collect();
    if chars.next().is_some() {
        let cut: String = kept.chars().take(PURPOSE_MAX_CHARS - 1).collect();
        Some(format!("{}…", cut.trim_end()))
    } else {
        Some(kept)
    }
}

fn markdown(text: &str) -> Option<String> {
    let lines = || text.lines().map(str::trim).filter(|line| !line.is_empty());
    lines()
        .find_map(|line| {
            line.strip_prefix('#')
                .map(|heading| heading.trim_start_matches('#').trim())
        })
        .filter(|heading| !heading.is_empty())
        .or_else(|| lines().next())
        .and_then(clean)
}

fn docstring(text: &str) -> Option<String> {
    let body: Vec<&str> = text
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty() || line.starts_with('#'))
        .collect();
    let first = body.first()?;
    let quote = ["\"\"\"", "'''"].into_iter().find(|quote| {
        first
            .trim_start_matches(['r', 'u', 'R', 'U'])
            .starts_with(quote)
    })?;
    let opened = first
        .trim_start_matches(['r', 'u', 'R', 'U'])
        .trim_start_matches(quote);
    std::iter::once(opened)
        .chain(body[1..].iter().copied())
        .map(|line| line.split(quote).next().unwrap_or_default().trim())
        .find(|line| !line.is_empty())
        .and_then(clean)
}

fn leading_comment(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("#!"))
        .take_while(|line| {
            line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
        })
        .map(|line| {
            line.trim_start_matches('/')
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .trim()
        })
        .find(|line| !line.is_empty())
        .and_then(clean)
}

fn described(text: &str) -> Option<String> {
    DESCRIPTION.captures(text).and_then(|captures| {
        captures
            .get(1)
            .or_else(|| captures.get(2))
            .and_then(|found| clean(found.as_str()))
    })
}

pub fn purpose_of(path: &str, file: &Path) -> Option<String> {
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let text = head(file)?;
    match extension.as_str() {
        "md" | "markdown" | "txt" => markdown(&text),
        "py" => docstring(&text).or_else(|| described(&text)),
        "js" | "mjs" | "cjs" => leading_comment(&text).or_else(|| described(&text)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testing::temporary_directory;

    fn purpose(path: &str, content: &[u8]) -> Option<String> {
        let folder = temporary_directory("studio-skill-purpose-");
        let file = folder.join("file");
        fs::write(&file, content).expect("writes");
        purpose_of(path, &file)
    }

    #[test]
    fn a_reference_is_described_by_its_first_heading_or_first_line() {
        assert_eq!(
            purpose(
                "references/spec.md",
                b"\n# Dashboard spec\n\nThe spec is JSON."
            ),
            Some("Dashboard spec".to_owned())
        );
        assert_eq!(
            purpose("references/notes.md", b"\n\nPlain first line\nsecond"),
            Some("Plain first line".to_owned())
        );
        let long = format!("# {}", "a".repeat(200));
        let described = purpose("references/long.md", long.as_bytes()).expect("a purpose");
        assert_eq!(described.chars().count(), PURPOSE_MAX_CHARS);
        assert!(described.ends_with('…'));
    }

    #[test]
    fn a_script_is_described_by_its_docstring_or_its_argparse_description() {
        assert_eq!(
            purpose(
                "scripts/a.py",
                b"# /// script\n# dependencies = []\n# ///\n\"\"\"\nProfile a data file.\n\nMore.\n\"\"\"\nimport sys\n"
            ),
            Some("Profile a data file.".to_owned())
        );
        assert_eq!(
            purpose(
                "scripts/b.py",
                b"import argparse\nparser = argparse.ArgumentParser(description=\"Build the page.\")\n"
            ),
            Some("Build the page.".to_owned())
        );
        assert_eq!(
            purpose(
                "scripts/c.mjs",
                b"#!/usr/bin/env node\n// Render a chart.\nexport {};\n"
            ),
            Some("Render a chart.".to_owned())
        );
        assert_eq!(purpose("scripts/_shared.py", b"import csv\n"), None);
    }

    #[test]
    fn binary_files_and_other_assets_have_no_purpose() {
        assert_eq!(purpose("assets/logo.png", b"\x89PNG\x00\x01"), None);
        assert_eq!(purpose("references/blob.md", b"# a\x00b"), None);
        assert_eq!(purpose("assets/template.html", b"<!doctype html>"), None);
    }
}
