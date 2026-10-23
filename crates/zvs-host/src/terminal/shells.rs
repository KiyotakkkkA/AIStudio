use std::path::{Path, PathBuf};

use zvs_contract::TerminalShell;

use crate::toolchain::process::{find_program, path_dirs};

pub const SHELL_ORDER: [TerminalShell; 5] = [
    TerminalShell::Pwsh,
    TerminalShell::PowerShell,
    TerminalShell::Cmd,
    TerminalShell::GitBash,
    TerminalShell::Wsl,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

pub trait ShellLocator: Send + Sync {
    fn locate(&self, shell: TerminalShell) -> Option<ShellCommand>;
}

pub const fn shell_title(shell: TerminalShell) -> &'static str {
    match shell {
        TerminalShell::PowerShell => "Windows PowerShell",
        TerminalShell::Pwsh => "PowerShell",
        TerminalShell::Cmd => "Командная строка",
        TerminalShell::GitBash => "Git Bash",
        TerminalShell::Wsl => "WSL",
    }
}

pub struct SystemShells {
    system_root: PathBuf,
    program_files: PathBuf,
    comspec: Option<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl SystemShells {
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Self {
            system_root: var("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
            program_files: var("ProgramFiles")
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files")),
            comspec: var("ComSpec"),
            dirs: path_dirs(std::env::var_os("PATH").as_ref()),
        }
    }

    fn on_path(&self, name: &str) -> Option<PathBuf> {
        let pathext = std::env::var("PATHEXT").ok();
        find_program(
            name,
            &self.dirs,
            Path::new(""),
            pathext.as_deref(),
            &|path| path.is_file(),
        )
    }

    fn system32(&self, name: &str) -> PathBuf {
        self.system_root.join("System32").join(name)
    }

    fn git_bash(&self) -> Option<PathBuf> {
        let from_git = self.on_path("git").and_then(|git| {
            git.parent()
                .and_then(Path::parent)
                .map(|root| root.join("bin").join("bash.exe"))
        });
        from_git
            .into_iter()
            .chain([self.program_files.join("Git").join("bin").join("bash.exe")])
            .find(|path| path.is_file())
    }
}

fn existing(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

fn command(program: PathBuf, args: &[&str]) -> ShellCommand {
    ShellCommand {
        program,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

impl ShellLocator for SystemShells {
    fn locate(&self, shell: TerminalShell) -> Option<ShellCommand> {
        if !cfg!(windows) {
            return None;
        }
        match shell {
            TerminalShell::PowerShell => existing(
                self.system32("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe"),
            )
            .map(|program| command(program, &["-NoLogo"])),
            TerminalShell::Pwsh => self
                .on_path("pwsh")
                .or_else(|| {
                    existing(
                        self.program_files
                            .join("PowerShell")
                            .join("7")
                            .join("pwsh.exe"),
                    )
                })
                .map(|program| command(program, &["-NoLogo"])),
            TerminalShell::Cmd => self
                .comspec
                .clone()
                .and_then(existing)
                .or_else(|| existing(self.system32("cmd.exe")))
                .map(|program| command(program, &[])),
            TerminalShell::GitBash => self
                .git_bash()
                .map(|program| command(program, &["--login", "-i"])),
            TerminalShell::Wsl => {
                existing(self.system32("wsl.exe")).map(|program| command(program, &[]))
            }
        }
    }
}
