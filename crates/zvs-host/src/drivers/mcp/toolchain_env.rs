use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainDirs {
    pub root: PathBuf,
    pub cache: Option<PathBuf>,
}

impl ToolchainDirs {
    pub fn new(root: PathBuf, cache: PathBuf) -> Self {
        Self {
            root,
            cache: Some(cache),
        }
    }

    pub fn inside(root: PathBuf) -> Self {
        Self { root, cache: None }
    }

    fn cache_path(&self, inside: &[&str], shared: &str) -> PathBuf {
        match &self.cache {
            Some(cache) => cache.join(shared),
            None => nested(&self.root, inside),
        }
    }
}

fn nested(root: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

pub fn isolated_toolchain_env(dirs: &ToolchainDirs) -> BTreeMap<String, String> {
    let text = |path: PathBuf| path.to_string_lossy().into_owned();
    let path = |parts: &[&str]| text(nested(&dirs.root, parts));
    BTreeMap::from([
        (
            "npm_config_cache".to_owned(),
            text(dirs.cache_path(&["npm-cache"], "npm")),
        ),
        ("npm_config_prefix".to_owned(), path(&["npm-global"])),
        ("npm_config_update_notifier".to_owned(), "false".to_owned()),
        ("npm_config_fund".to_owned(), "false".to_owned()),
        (
            "UV_CACHE_DIR".to_owned(),
            text(dirs.cache_path(&["uv", "cache"], "uv")),
        ),
        ("UV_TOOL_DIR".to_owned(), path(&["uv", "tools"])),
        ("UV_TOOL_BIN_DIR".to_owned(), path(&["uv", "bin"])),
        ("UV_PYTHON_INSTALL_DIR".to_owned(), path(&["uv", "python"])),
        ("UV_PYTHON_PREFERENCE".to_owned(), "only-managed".to_owned()),
        (
            "PIP_CACHE_DIR".to_owned(),
            text(dirs.cache_path(&["pip-cache"], "pip")),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toolchain_env_keeps_npx_and_uvx_downloads_inside_the_app_folder() {
        let root = Path::new("C:").join("assets").join("toolchain");
        let isolated = isolated_toolchain_env(&ToolchainDirs::inside(root.clone()));
        let root = root.to_string_lossy().into_owned();
        for name in [
            "npm_config_cache",
            "UV_CACHE_DIR",
            "UV_TOOL_DIR",
            "UV_PYTHON_INSTALL_DIR",
        ] {
            assert!(isolated[name].starts_with(&root), "{name}");
        }
        assert_eq!(isolated["UV_PYTHON_PREFERENCE"], "only-managed");
    }

    #[test]
    fn package_caches_leave_the_installation_when_a_cache_root_is_given() {
        let root = Path::new("C:").join("assets").join("toolchain");
        let cache = Path::new("C:").join("cache").join("packages");
        let isolated = isolated_toolchain_env(&ToolchainDirs::new(root.clone(), cache.clone()));
        let text = |path: PathBuf| path.to_string_lossy().into_owned();
        assert_eq!(isolated["npm_config_cache"], text(cache.join("npm")));
        assert_eq!(isolated["UV_CACHE_DIR"], text(cache.join("uv")));
        assert_eq!(isolated["PIP_CACHE_DIR"], text(cache.join("pip")));
        assert_eq!(isolated["npm_config_prefix"], text(root.join("npm-global")));
        assert_eq!(
            isolated["UV_PYTHON_INSTALL_DIR"],
            text(root.join("uv").join("python"))
        );
    }
}
