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
    let name = [".exe", ".cmd", ".bat", ".com"]
        .iter()
        .find_map(|extension| name.strip_suffix(extension))
        .unwrap_or(&name)
        .to_owned();
    (!name.is_empty()).then_some(name)
}

fn python(stem: &str) -> bool {
    stem.starts_with("python") || matches!(stem, "py" | "pyw")
}

fn interpreter(stem: &str) -> Option<CodeLanguage> {
    if python(stem) || stem == "uv" {
        return Some(CodeLanguage::Python);
    }
    match stem {
        "node" | "deno" | "bun" | "tsx" => Some(CodeLanguage::Javascript),
        "pwsh" | "powershell" => Some(CodeLanguage::Powershell),
        "bash" | "sh" | "zsh" => Some(CodeLanguage::Shell),
        _ => None,
    }
}

fn extensions(language: CodeLanguage) -> &'static [&'static str] {
    match language {
        CodeLanguage::Python => &["py", "pyw"],
        CodeLanguage::Javascript => &["js", "mjs", "cjs", "ts", "mts", "cts"],
        CodeLanguage::Powershell => &["ps1"],
        CodeLanguage::Shell => &["sh", "bash", "zsh"],
    }
}

fn has_extension(argument: &str, known: &[&str]) -> bool {
    Path::new(argument)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            known
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

fn is_script(argument: &str, language: CodeLanguage) -> bool {
    !argument.starts_with('-') && has_extension(argument, extensions(language))
}

const POWERSHELL_STRINGS: [&str; 3] = ["command", "encodedcommand", "commandwithargs"];
const POWERSHELL_ALIASES: [&str; 2] = ["ec", "cwa"];
const POWERSHELL_VALUES: [&str; 11] = [
    "executionpolicy",
    "windowstyle",
    "configurationname",
    "version",
    "outputformat",
    "inputformat",
    "workingdirectory",
    "settingsfile",
    "psconsolefile",
    "custompipename",
    "encodedarguments",
];
const POWERSHELL_VALUE_ALIASES: [&str; 8] = ["ex", "ep", "w", "v", "o", "of", "wd", "ea"];

fn powershell_flag(argument: &str) -> Option<String> {
    let name = argument
        .strip_prefix('-')
        .or_else(|| argument.strip_prefix('/'))?;
    if name.contains(['/', '\\']) {
        return None;
    }
    let name = name.trim_start_matches('-');
    Some(
        name.split(':')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

fn powershell_string(rest: &[&str], windows: bool) -> bool {
    let mut arguments = rest.iter();
    while let Some(argument) = arguments.next() {
        if *argument == "-" {
            return false;
        }
        let Some(name) = powershell_flag(argument) else {
            return windows && !has_extension(argument, &["ps1"]);
        };
        if name.is_empty() {
            continue;
        }
        if POWERSHELL_ALIASES.contains(&name.as_str())
            || POWERSHELL_STRINGS
                .iter()
                .any(|flag| flag.starts_with(name.as_str()))
        {
            return true;
        }
        if "file".starts_with(name.as_str()) {
            return false;
        }
        let valued = POWERSHELL_VALUE_ALIASES.contains(&name.as_str())
            || (name.len() > 1
                && POWERSHELL_VALUES
                    .iter()
                    .any(|flag| flag.starts_with(name.as_str())));
        if valued && !argument.contains(':') {
            arguments.next();
        }
    }
    false
}

fn slash_switch(rest: &[&str], switches: &[char]) -> bool {
    rest.iter().any(|argument| {
        let mut chars = argument.chars();
        chars.next() == Some('/')
            && chars
                .next()
                .is_some_and(|switch| switches.contains(&switch.to_ascii_lowercase()))
    })
}

const WSL_READ_ONLY: [&str; 13] = [
    "--version",
    "-v",
    "--help",
    "--status",
    "--list",
    "-l",
    "--verbose",
    "--running",
    "--quiet",
    "-q",
    "--all",
    "--online",
    "-o",
];

fn wsl_runs(rest: &[&str]) -> bool {
    rest.is_empty()
        || rest
            .iter()
            .any(|argument| !WSL_READ_ONLY.contains(&argument.to_ascii_lowercase().as_str()))
}

struct Options {
    short: &'static [char],
    stop: &'static [char],
    long: &'static [&'static str],
    valued: &'static [&'static str],
    subcommands: &'static [&'static str],
}

const SHELL_OPTIONS: Options = Options {
    short: &['c'],
    stop: &[],
    long: &[],
    valued: &["-o", "-O", "--rcfile", "--init-file"],
    subcommands: &[],
};

const PYTHON_OPTIONS: Options = Options {
    short: &['c'],
    stop: &['m'],
    long: &[],
    valued: &["-W", "-X"],
    subcommands: &[],
};

const JAVASCRIPT_OPTIONS: Options = Options {
    short: &['e', 'p'],
    stop: &[],
    long: &["eval", "print"],
    valued: &[
        "-r",
        "--require",
        "--import",
        "--loader",
        "--experimental-loader",
        "-C",
        "--conditions",
        "--input-type",
        "--env-file",
        "--title",
    ],
    subcommands: &["eval"],
};

const INLINE_E_OPTIONS: Options = Options {
    short: &['e', 'E'],
    stop: &[],
    long: &[],
    valued: &["-I", "-r"],
    subcommands: &[],
};

const PHP_OPTIONS: Options = Options {
    short: &['r'],
    stop: &[],
    long: &[],
    valued: &["-c", "-d"],
    subcommands: &[],
};

fn options(stem: &str) -> Option<&'static Options> {
    if python(stem) {
        return Some(&PYTHON_OPTIONS);
    }
    match stem {
        "bash" | "sh" | "zsh" | "dash" | "ksh" | "fish" => Some(&SHELL_OPTIONS),
        "node" | "deno" | "bun" | "tsx" => Some(&JAVASCRIPT_OPTIONS),
        "ruby" | "perl" | "lua" | "osascript" => Some(&INLINE_E_OPTIONS),
        "php" => Some(&PHP_OPTIONS),
        _ => None,
    }
}

fn option_string(rest: &[&str], options: &Options) -> bool {
    let mut arguments = rest.iter();
    while let Some(argument) = arguments.next() {
        if *argument == "--" || *argument == "-" {
            return false;
        }
        if let Some(name) = argument.strip_prefix("--") {
            let (name, inline) = name
                .split_once('=')
                .map_or((name, false), |(name, _)| (name, true));
            if options.long.contains(&name) {
                return true;
            }
            if !inline && options.valued.contains(argument) {
                arguments.next();
            }
            continue;
        }
        let Some(flags) = argument.strip_prefix('-') else {
            return options.subcommands.contains(argument);
        };
        if flags.chars().all(|flag| flag.is_ascii_alphabetic()) {
            for flag in flags.chars() {
                if options.short.contains(&flag) {
                    return true;
                }
                if options.stop.contains(&flag) {
                    return false;
                }
            }
        }
        if options.valued.contains(argument) {
            arguments.next();
        }
    }
    false
}

pub fn script_string(argv: &[String]) -> bool {
    let Some((program, rest)) = argv.split_first() else {
        return false;
    };
    let Some(stem) = program_stem(program) else {
        return false;
    };
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    match stem.as_str() {
        "powershell" => powershell_string(&rest, true),
        "pwsh" => powershell_string(&rest, false),
        "cmd" => slash_switch(&rest, &['c', 'k', 'r']),
        "forfiles" => slash_switch(&rest, &['c']),
        "wsl" => wsl_runs(&rest),
        _ => options(&stem).is_some_and(|options| option_string(&rest, options)),
    }
}

pub fn interpreter_program(program: &str) -> bool {
    program_stem(program).is_some_and(|stem| {
        interpreter(&stem).is_some()
            || options(&stem).is_some()
            || matches!(
                stem.as_str(),
                "cmd"
                    | "powershell"
                    | "pwsh"
                    | "wsl"
                    | "forfiles"
                    | "cscript"
                    | "wscript"
                    | "mshta"
                    | "rundll32"
                    | "regsvr32"
                    | "env"
                    | "xargs"
                    | "start"
            )
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
    if script_string(argv) {
        return None;
    }
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
        std::fs::write(dir.path().join("_work/build.sh"), "rm -rf dist\nmake all\n")
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
    fn a_shell_script_file_is_read_like_the_other_interpreters() {
        let dir = folder();
        for program in ["bash", "sh", "C:/Program Files/Git/bin/zsh.exe"] {
            let found = script_source(&argv(&[program, "_work/build.sh"]), dir.path(), None)
                .expect("a script");
            assert_eq!(found.language, CodeLanguage::Shell, "{program}");
            assert!(found.source.contains("rm -rf dist"), "{program}");
        }
        let piped = script_source(&argv(&["sh", "-s"]), dir.path(), Some("ls -la"))
            .expect("a script on standard input");
        assert_eq!(piped.language, CodeLanguage::Shell);
        assert_eq!(piped.source, "ls -la");
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

    #[test]
    fn a_script_string_carries_no_file_source() {
        let dir = folder();
        assert_eq!(
            script_source(
                &argv(&["python", "-c", "x", "_work/main.py"]),
                dir.path(),
                None
            ),
            None
        );
        assert_eq!(
            script_source(&argv(&["bash", "-c", "_work/build.sh"]), dir.path(), None),
            None
        );
    }

    #[test]
    fn every_powershell_spelling_of_command_and_encoded_command_is_a_script_string() {
        for parts in [
            &["powershell", "-Command", "Get-Date"][..],
            &["powershell.exe", "-c", "Get-Date"],
            &["powershell", "-com", "Get-Date"],
            &["powershell", "-comm", "Get-Date"],
            &["powershell", "-ec", "RwBlAHQALQBEAGEAdABlAA=="],
            &["powershell", "-e", "RwBlAHQALQBEAGEAdABlAA=="],
            &["powershell", "-en", "RwBlAHQALQBEAGEAdABlAA=="],
            &["powershell", "-enco", "RwBlAHQALQBEAGEAdABlAA=="],
            &["powershell", "-EncodedCommand", "RwBlAHQALQBEAGEAdABlAA=="],
            &["powershell", "/Command", "Get-Date"],
            &["pwsh", "--command", "Get-Date"],
            &[
                "pwsh",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-cwa",
                "$args",
                "1",
            ],
            &[
                "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
                "-NoP",
                "-W",
                "Hidden",
                "-enc",
                "AA==",
            ],
        ] {
            assert!(script_string(&argv(parts)), "{parts:?}");
        }
    }

    #[test]
    fn a_bare_positional_command_is_a_script_string_for_windows_powershell_only() {
        assert!(script_string(&argv(&["powershell", "Get-ChildItem"])));
        assert!(script_string(&argv(&[
            "powershell",
            "-NoProfile",
            "Remove-Item -Recurse C:/data",
        ])));
        assert!(!script_string(&argv(&["pwsh", "Get-ChildItem"])));
        assert!(!script_string(&argv(&["powershell", "build.ps1"])));
        assert!(!script_string(&argv(&[
            "powershell",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "build.ps1",
        ])));
    }

    #[test]
    fn cmd_takes_slash_c_k_and_r_in_either_case() {
        for parts in [
            &["cmd", "/c", "dir"][..],
            &["cmd.exe", "/C", "dir"],
            &["cmd", "/k", "dir"],
            &["cmd", "/r", "del", "x"],
            &["cmd", "/R", "del", "x"],
            &["cmd", "/s", "/q", "/c", "dir"],
            &["forfiles", "/p", "C:/work", "/c", "cmd /c del @file"],
        ] {
            assert!(script_string(&argv(parts)), "{parts:?}");
        }
        assert!(!script_string(&argv(&["cmd"])));
    }

    #[test]
    fn wsl_running_a_program_is_a_script_string_and_its_listings_are_not() {
        for parts in [
            &["wsl", "rm", "-rf", "/mnt/c/work"][..],
            &["wsl.exe", "-d", "Ubuntu", "--", "ls"],
            &["wsl", "-e", "bash"],
            &["wsl"],
        ] {
            assert!(script_string(&argv(parts)), "{parts:?}");
        }
        for parts in [&["wsl", "--list", "--verbose"][..], &["wsl", "--status"]] {
            assert!(!script_string(&argv(parts)), "{parts:?}");
        }
    }

    #[test]
    fn other_interpreters_take_their_inline_flags_before_the_first_argument() {
        for parts in [
            &["bash", "-c", "rm -rf dist"][..],
            &["bash", "-lc", "make"],
            &["sh", "-xc", "ls"],
            &["zsh", "-c", "ls"],
            &["python", "-c", "print(1)"],
            &["python3", "-Ic", "print(1)"],
            &["py", "-c", "print(1)"],
            &["node", "-e", "1"],
            &["node", "-pe", "1"],
            &["node", "--eval=1"],
            &["node", "--print", "1"],
            &["bun", "--eval", "1"],
            &["deno", "eval", "console.log(1)"],
            &["perl", "-ne", "print"],
            &["ruby", "-e", "puts 1"],
            &["php", "-r", "echo 1;"],
        ] {
            assert!(script_string(&argv(parts)), "{parts:?}");
        }
        for parts in [
            &["python", "-m", "pytest", "-c", "pytest.ini"][..],
            &["python", "main.py", "-c"],
            &["node", "script.js", "-e"],
            &["bash", "build.sh", "-c"],
            &["node", "--version"],
            &["git", "-c", "core.pager=", "log"],
            &["cargo", "test"],
        ] {
            assert!(!script_string(&argv(parts)), "{parts:?}");
        }
    }
}
