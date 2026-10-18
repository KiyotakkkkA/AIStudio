use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
const META: &[char] = &[
    '(', ')', ']', '[', '%', '!', '^', '"', '`', '<', '>', '&', '|', ';', ',', ' ', '*', '?',
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsLaunch {
    pub file: String,
    pub args: Vec<String>,
    pub verbatim: bool,
}

pub fn env_value<'a>(env: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    env.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

pub fn is_file(path: &Path) -> bool {
    path.is_file()
}

fn extension_of(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let index = name.rfind('.')?;
    (index > 0).then(|| name[index..].to_owned())
}

fn is_absolute(command: &str) -> bool {
    let bytes = command.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        && (bytes[2] == b'\\' || bytes[2] == b'/')
        || command.starts_with("\\\\")
        || Path::new(command).is_absolute()
}

fn join(directory: &str, name: &str) -> String {
    PathBuf::from(directory)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

pub fn resolve_windows_command(
    command: &str,
    env: &HashMap<String, String>,
    exists: &dyn Fn(&Path) -> bool,
    cwd: &Path,
) -> Option<String> {
    let extensions: Vec<String> = env_value(env, "PATHEXT")
        .unwrap_or(DEFAULT_PATHEXT)
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect();
    let candidates = |base: &str| -> Vec<String> {
        let known = extension_of(base).is_some_and(|extension| {
            extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
        });
        if known {
            return vec![base.to_owned()];
        }
        let mut list: Vec<String> = extensions
            .iter()
            .map(|extension| format!("{base}{}", extension.to_lowercase()))
            .collect();
        list.push(base.to_owned());
        list
    };
    let cwd = cwd.to_string_lossy().into_owned();
    if command.contains(['\\', '/']) || is_absolute(command) {
        let base = if is_absolute(command) {
            command.to_owned()
        } else {
            join(&cwd, command)
        };
        return candidates(&base)
            .into_iter()
            .find(|path| exists(Path::new(path)));
    }
    let directories: Vec<String> = env_value(env, "PATH")
        .unwrap_or_default()
        .split(';')
        .map(|entry| {
            let entry = entry.trim();
            entry
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .unwrap_or(entry)
                .to_owned()
        })
        .filter(|entry| !entry.is_empty())
        .collect();
    std::iter::once(cwd)
        .chain(directories)
        .find_map(|directory| {
            candidates(&join(&directory, command))
                .into_iter()
                .find(|path| exists(Path::new(path)))
        })
}

fn caret(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if META.contains(&character) {
            escaped.push('^');
        }
        escaped.push(character);
    }
    escaped
}

pub fn escape_argument(argument: &str) -> String {
    let mut quoted = String::with_capacity(argument.len() + 2);
    let mut backslashes = 0;
    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2));
                quoted.push_str("\\\"");
                backslashes = 0;
                continue;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                quoted.push(character);
                continue;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    caret(&format!("\"{quoted}\""))
}

pub fn windows_launch(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    exists: &dyn Fn(&Path) -> bool,
    cwd: &Path,
) -> WindowsLaunch {
    let resolved =
        resolve_windows_command(command, env, exists, cwd).unwrap_or_else(|| command.to_owned());
    let lower = resolved.to_lowercase();
    if !(lower.ends_with(".cmd") || lower.ends_with(".bat")) {
        return WindowsLaunch {
            file: resolved,
            args: args.to_vec(),
            verbatim: false,
        };
    }
    let line = std::iter::once(caret(&resolved))
        .chain(args.iter().map(|argument| escape_argument(argument)))
        .collect::<Vec<_>>()
        .join(" ");
    WindowsLaunch {
        file: env_value(env, "COMSPEC").unwrap_or("cmd.exe").to_owned(),
        args: vec![
            "/d".to_owned(),
            "/s".to_owned(),
            "/c".to_owned(),
            format!("\"{line}\""),
        ],
        verbatim: true,
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const COMSPEC: &str = "C:\\Windows\\system32\\cmd.exe";

    fn node_dir() -> String {
        join("C:\\", "Program Files\\nodejs")
    }

    fn tools_dir() -> String {
        "C:\\tools".to_owned()
    }

    fn files() -> HashSet<String> {
        [
            join(&node_dir(), "npx.cmd"),
            join(&node_dir(), "npx"),
            join(&node_dir(), "node.exe"),
            join(&tools_dir(), "uvx.exe"),
        ]
        .into_iter()
        .collect()
    }

    fn env() -> HashMap<String, String> {
        HashMap::from([
            ("Path".to_owned(), format!("{};{}", node_dir(), tools_dir())),
            ("PATHEXT".to_owned(), ".COM;.EXE;.BAT;.CMD".to_owned()),
        ])
    }

    fn work() -> PathBuf {
        PathBuf::from("C:\\work")
    }

    #[test]
    fn a_bare_command_resolves_through_path_and_pathext_preferring_the_shim_over_the_bash_script() {
        let files = files();
        let exists = |path: &Path| files.contains(&path.to_string_lossy().into_owned());
        assert_eq!(
            resolve_windows_command("npx", &env(), &exists, &work()),
            Some(join(&node_dir(), "npx.cmd"))
        );
        assert_eq!(
            resolve_windows_command("uvx", &env(), &exists, &work()),
            Some(join(&tools_dir(), "uvx.exe"))
        );
        assert_eq!(
            resolve_windows_command("node.exe", &env(), &exists, &work()),
            Some(join(&node_dir(), "node.exe"))
        );
        assert_eq!(
            resolve_windows_command("missing", &env(), &exists, &work()),
            None
        );
    }

    #[test]
    fn an_executable_is_spawned_directly_with_its_arguments_untouched() {
        let files = files();
        let exists = |path: &Path| files.contains(&path.to_string_lossy().into_owned());
        assert_eq!(
            windows_launch(
                "uvx",
                &["mcp-server-time".to_owned()],
                &env(),
                &exists,
                &work()
            ),
            WindowsLaunch {
                file: join(&tools_dir(), "uvx.exe"),
                args: vec!["mcp-server-time".to_owned()],
                verbatim: false,
            }
        );
    }

    #[test]
    fn a_cmd_shim_runs_through_cmd_exe_with_every_argument_quoted_and_metacharacters_escaped() {
        let files = files();
        let exists = |path: &Path| files.contains(&path.to_string_lossy().into_owned());
        let mut env = env();
        env.insert("ComSpec".to_owned(), COMSPEC.to_owned());
        let args: Vec<String> = [
            "-y",
            "@modelcontextprotocol/server-filesystem",
            "C:\\My Files",
            "a\"b",
            "x&calc",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let launch = windows_launch("npx", &args, &env, &exists, &work());
        assert_eq!(launch.file, COMSPEC);
        assert!(launch.verbatim);
        assert_eq!(launch.args[..3], ["/d", "/s", "/c"]);
        let line = &launch.args[3];
        assert!(line.starts_with('"') && line.ends_with('"'));
        assert!(line.contains("npx.cmd"));
        assert!(line.contains("^\"@modelcontextprotocol/server-filesystem^\""));
        assert!(line.contains("^\"C:\\My^ Files^\""));
        assert!(line.contains("^\"a\\^\"b^\""));
        assert!(line.contains("^\"x^&calc^\""));
        let bytes = line.as_bytes();
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| *byte != b'&' || (index > 0 && bytes[index - 1] == b'^'))
        );
    }

    #[test]
    fn trailing_backslashes_are_doubled_before_the_closing_quote() {
        assert_eq!(escape_argument("C:\\dir\\"), "^\"C:\\dir\\\\^\"");
    }
}
