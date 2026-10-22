use std::fmt;

use zvs_contract::ToolchainRuntime;

use crate::runtimes::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let core = text.split(['-', '+', ' ']).next()?;
        let mut parts = core.split('.');
        let mut number = || -> Option<u64> {
            let part = parts.next()?;
            (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        };
        let version = Self::new(number()?, number()?, number()?);
        parts.next().is_none().then_some(version)
    }

    pub fn find(output: &str) -> Option<Self> {
        output
            .split_whitespace()
            .find_map(|token| Self::parse(token.trim_matches(|c: char| c == '(' || c == ')')))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolchainDefinition {
    pub runtime: ToolchainRuntime,
    pub display_name: &'static str,
    pub description: &'static str,
    pub minimum: Version,
    pub approximate_bytes: u64,
    pub page: &'static str,
    pub banner: &'static str,
}

pub const TOOLCHAIN_DEFINITIONS: [ToolchainDefinition; 2] = [
    ToolchainDefinition {
        runtime: ToolchainRuntime::Uv,
        display_name: "uv",
        description: "Менеджер пакетов Python. Нужен MCP-серверам, которые запускаются через uvx.",
        minimum: Version::new(0, 4, 0),
        approximate_bytes: 20 * 1024 * 1024,
        page: "https://github.com/astral-sh/uv/releases",
        banner: "uv",
    },
    ToolchainDefinition {
        runtime: ToolchainRuntime::Node,
        display_name: "Node.js",
        description: "Среда выполнения JavaScript. Нужна MCP-серверам, которые запускаются через npx.",
        minimum: Version::new(18, 0, 0),
        approximate_bytes: 35 * 1024 * 1024,
        page: "https://nodejs.org/en/download",
        banner: "v",
    },
];

pub fn definition(runtime: ToolchainRuntime) -> &'static ToolchainDefinition {
    match runtime {
        ToolchainRuntime::Uv => &TOOLCHAIN_DEFINITIONS[0],
        ToolchainRuntime::Node => &TOOLCHAIN_DEFINITIONS[1],
    }
}

pub fn is_windows(platform: Platform) -> bool {
    platform.os == "win32"
}

pub fn primary_name(runtime: ToolchainRuntime, platform: Platform) -> &'static str {
    match (runtime, is_windows(platform)) {
        (ToolchainRuntime::Uv, true) => "uv.exe",
        (ToolchainRuntime::Uv, false) => "uv",
        (ToolchainRuntime::Node, true) => "node.exe",
        (ToolchainRuntime::Node, false) => "node",
    }
}

pub fn companion_name(runtime: ToolchainRuntime, platform: Platform) -> &'static str {
    match (runtime, is_windows(platform)) {
        (ToolchainRuntime::Uv, true) => "uvx.exe",
        (ToolchainRuntime::Uv, false) => "uvx",
        (ToolchainRuntime::Node, true) => "npx.cmd",
        (ToolchainRuntime::Node, false) => "npx",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainCommand {
    Node,
    Npx,
    Uv,
    Uvx,
}

impl ToolchainCommand {
    pub fn parse(command: &str) -> Option<Self> {
        let command = command.trim();
        if command.contains(['/', '\\']) {
            return None;
        }
        let lower = command.to_ascii_lowercase();
        let bare = [".exe", ".cmd"]
            .iter()
            .find_map(|suffix| lower.strip_suffix(suffix))
            .unwrap_or(&lower);
        match bare {
            "node" => Some(Self::Node),
            "npx" => Some(Self::Npx),
            "uv" => Some(Self::Uv),
            "uvx" => Some(Self::Uvx),
            _ => None,
        }
    }

    pub const fn runtime(self) -> ToolchainRuntime {
        match self {
            Self::Node | Self::Npx => ToolchainRuntime::Node,
            Self::Uv | Self::Uvx => ToolchainRuntime::Uv,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Npx => "npx",
            Self::Uv => "uv",
            Self::Uvx => "uvx",
        }
    }

    pub fn file_name(self, platform: Platform) -> &'static str {
        match self {
            Self::Node | Self::Uv => primary_name(self.runtime(), platform),
            Self::Npx | Self::Uvx => companion_name(self.runtime(), platform),
        }
    }
}

pub fn node_file_key(platform: Platform) -> Option<String> {
    let arch = match platform.arch {
        "x64" | "arm64" => platform.arch,
        _ => return None,
    };
    match platform.os {
        "win32" => Some(format!("win-{arch}-zip")),
        "linux" => Some(format!("linux-{arch}")),
        "darwin" => Some(format!("osx-{arch}-tar")),
        _ => None,
    }
}

pub fn node_asset_name(version: &str, platform: Platform) -> Option<String> {
    node_file_key(platform)?;
    Some(match platform.os {
        "win32" => format!("node-{version}-win-{}.zip", platform.arch),
        "linux" => format!("node-{version}-linux-{}.tar.gz", platform.arch),
        _ => format!("node-{version}-darwin-{}.tar.gz", platform.arch),
    })
}

pub fn uv_asset_name(platform: Platform) -> Option<&'static str> {
    Some(match (platform.os, platform.arch) {
        ("win32", "x64") => "uv-x86_64-pc-windows-msvc.zip",
        ("win32", "arm64") => "uv-aarch64-pc-windows-msvc.zip",
        ("linux", "x64") => "uv-x86_64-unknown-linux-gnu.tar.gz",
        ("linux", "arm64") => "uv-aarch64-unknown-linux-gnu.tar.gz",
        ("darwin", "x64") => "uv-x86_64-apple-darwin.tar.gz",
        ("darwin", "arm64") => "uv-aarch64-apple-darwin.tar.gz",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: Platform = Platform::new("win32", "x64");
    const LINUX: Platform = Platform::new("linux", "arm64");

    #[test]
    fn versions_parse_from_tags_and_from_version_banners() {
        assert_eq!(Version::parse("v24.11.0"), Some(Version::new(24, 11, 0)));
        assert_eq!(Version::parse("0.9.5"), Some(Version::new(0, 9, 5)));
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("1.x.3"), None);
        assert_eq!(
            Version::find("uv 0.9.5 (d5f39331a 2025-10-21)"),
            Some(Version::new(0, 9, 5))
        );
        assert_eq!(Version::find("v22.11.0\n"), Some(Version::new(22, 11, 0)));
        assert!(Version::new(18, 0, 0) > Version::new(16, 20, 2));
        assert!(Version::new(0, 10, 0) > Version::new(0, 9, 30));
    }

    #[test]
    fn only_bare_runtime_commands_map_to_the_toolchain() {
        assert_eq!(ToolchainCommand::parse("npx"), Some(ToolchainCommand::Npx));
        assert_eq!(
            ToolchainCommand::parse("NPX.cmd"),
            Some(ToolchainCommand::Npx)
        );
        assert_eq!(
            ToolchainCommand::parse("node.exe"),
            Some(ToolchainCommand::Node)
        );
        assert_eq!(ToolchainCommand::parse("uvx"), Some(ToolchainCommand::Uvx));
        assert_eq!(ToolchainCommand::parse("uv"), Some(ToolchainCommand::Uv));
        assert_eq!(ToolchainCommand::parse("python"), None);
        assert_eq!(ToolchainCommand::parse("C:\\nodejs\\npx.cmd"), None);
        assert_eq!(ToolchainCommand::parse("./uvx"), None);
        assert_eq!(ToolchainCommand::Npx.file_name(WINDOWS), "npx.cmd");
        assert_eq!(ToolchainCommand::Uvx.file_name(LINUX), "uvx");
    }

    #[test]
    fn assets_are_named_per_platform() {
        assert_eq!(
            node_asset_name("v24.11.0", WINDOWS).as_deref(),
            Some("node-v24.11.0-win-x64.zip")
        );
        assert_eq!(
            node_asset_name("v24.11.0", LINUX).as_deref(),
            Some("node-v24.11.0-linux-arm64.tar.gz")
        );
        assert_eq!(node_file_key(WINDOWS).as_deref(), Some("win-x64-zip"));
        assert_eq!(
            node_asset_name("v1.0.0", Platform::new("win32", "ia32")),
            None
        );
        assert_eq!(
            uv_asset_name(WINDOWS),
            Some("uv-x86_64-pc-windows-msvc.zip")
        );
        assert_eq!(uv_asset_name(Platform::new("freebsd", "x64")), None);
    }
}
