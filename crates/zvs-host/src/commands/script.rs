use std::path::Path;

use zvs_contract::{CodeLanguage, PROCESS_SOURCE_MAX};

use crate::policy::UNREAD_SCRIPT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSource {
    pub language: CodeLanguage,
    pub source: String,
}

fn program_stem(program: &str) -> Option<String> {
    let name = program.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name).to_owned();
    (!name.is_empty()).then_some(name)
}

fn interpreter(stem: &str) -> Option<CodeLanguage> {
    if stem.starts_with("python") || matches!(stem, "py" | "pyw" | "uv") {
        return Some(CodeLanguage::Python);
    }
    match stem {
        "node" | "deno" | "bun" | "tsx" => Some(CodeLanguage::Javascript),
        "pwsh" | "powershell" => Some(CodeLanguage::Powershell),
        _ => None,
    }
}

fn extensions(language: CodeLanguage) -> &'static [&'static str] {
    match language {
        CodeLanguage::Python => &["py", "pyw"],
        CodeLanguage::Javascript => &["js", "mjs", "cjs", "ts", "mts", "cts"],
        CodeLanguage::Powershell => &["ps1"],
    }
}

fn is_script(argument: &str, language: CodeLanguage) -> bool {
    !argument.starts_with('-')
        && Path::new(argument)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extensions(language)
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            })
}

fn read_inside(path: &Path, cwd: &Path) -> Option<String> {
    let root = cwd.canonicalize().ok()?;
    let file = path.canonicalize().ok()?;
    if !file.starts_with(&root) || !file.is_file() {
        return None;
    }
    let size = file.metadata().ok()?.len();
    if size > PROCESS_SOURCE_MAX as u64 {
        return None;
    }
    std::fs::read_to_string(&file).ok()
}

pub fn script_source(argv: &[String], cwd: &Path, stdin: Option<&str>) -> Option<ScriptSource> {
    let stem = program_stem(argv.first()?)?;
    let language = interpreter(&stem)?;
    let rest = &argv[1..];
    if stem == "uv" && rest.first().map(String::as_str) != Some("run") {
        return None;
    }
    let source = match rest.iter().find(|argument| is_script(argument, language)) {
        Some(script) => {
            read_inside(&cwd.join(script), cwd).unwrap_or_else(|| UNREAD_SCRIPT.to_owned())
        }
        None => {
            let stdin = stdin.filter(|text| !text.trim().is_empty())?;
            if stdin.chars().count() > PROCESS_SOURCE_MAX {
                UNREAD_SCRIPT.to_owned()
            } else {
                stdin.to_owned()
            }
        }
    };
    Some(ScriptSource { language, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a folder");
        std::fs::create_dir_all(dir.path().join("_work")).expect("creates");
        std::fs::write(
            dir.path().join("_work/clean.mjs"),
            "import fs from 'node:fs';\nfs.rmSync('C:/data', { recursive: true });\n",
        )
        .expect("writes");
        std::fs::write(dir.path().join("_work/main.py"), "print(sum(range(10)))\n")
            .expect("writes");
        dir
    }

    #[test]
    fn an_interpreter_on_a_script_file_carries_the_file_text() {
        let dir = folder();
        let found =
            script_source(&argv(&["node", "_work/clean.mjs"]), dir.path(), None).expect("a script");
        assert_eq!(found.language, CodeLanguage::Javascript);
        assert!(found.source.contains("rmSync"));
        let found = script_source(
            &argv(&["C:\\Tools\\uv.exe", "run", "_work/main.py"]),
            dir.path(),
            None,
        )
        .expect("a script");
        assert_eq!(found.language, CodeLanguage::Python);
        assert_eq!(found.source, "print(sum(range(10)))\n");
    }

    #[test]
    fn code_on_standard_input_is_the_source() {
        let dir = folder();
        let found = script_source(
            &argv(&["python", "-"]),
            dir.path(),
            Some("import shutil\nshutil.rmtree('C:/data')"),
        )
        .expect("a script");
        assert!(found.source.contains("shutil.rmtree"));
    }

    #[test]
    fn a_script_outside_the_run_folder_or_missing_is_unread() {
        let dir = folder();
        let outside = tempfile::tempdir().expect("a folder");
        let path = outside.path().join("x.py");
        std::fs::write(&path, "print(1)").expect("writes");
        let found = script_source(
            &argv(&["python", &path.to_string_lossy()]),
            dir.path(),
            None,
        )
        .expect("a script");
        assert_eq!(found.source, UNREAD_SCRIPT);
        let found =
            script_source(&argv(&["python", "nope.py"]), dir.path(), None).expect("a script");
        assert_eq!(found.source, UNREAD_SCRIPT);
    }

    #[test]
    fn commands_without_a_script_are_left_to_the_argv_rules() {
        let dir = folder();
        for parts in [
            &["node", "--version"][..],
            &["uv", "pip", "install", "x.py"],
            &["uv", "run", "pytest"],
            &["python", "-m", "pytest"],
            &["cargo", "run", "main.py"],
            &["python", "-"],
        ] {
            assert_eq!(
                script_source(&argv(parts), dir.path(), None),
                None,
                "{parts:?}"
            );
        }
    }
}
