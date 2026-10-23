use std::collections::BTreeMap;
use std::path::Path;

pub const TOOLCHAIN_DIR: &str = "mcp-toolchain";

pub fn isolated_toolchain_env(root: &Path) -> BTreeMap<String, String> {
    let path = |parts: &[&str]| {
        parts
            .iter()
            .fold(root.to_path_buf(), |path, part| path.join(part))
            .to_string_lossy()
            .into_owned()
    };
    BTreeMap::from([
        ("npm_config_cache".to_owned(), path(&["npm-cache"])),
        ("npm_config_prefix".to_owned(), path(&["npm-global"])),
        ("npm_config_update_notifier".to_owned(), "false".to_owned()),
        ("npm_config_fund".to_owned(), "false".to_owned()),
        ("UV_CACHE_DIR".to_owned(), path(&["uv", "cache"])),
        ("UV_TOOL_DIR".to_owned(), path(&["uv", "tools"])),
        ("UV_TOOL_BIN_DIR".to_owned(), path(&["uv", "bin"])),
        ("UV_PYTHON_INSTALL_DIR".to_owned(), path(&["uv", "python"])),
        ("UV_PYTHON_PREFERENCE".to_owned(), "only-managed".to_owned()),
        ("PIP_CACHE_DIR".to_owned(), path(&["pip-cache"])),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toolchain_env_keeps_npx_and_uvx_downloads_inside_the_app_folder() {
        let root = Path::new("C:").join("data").join(TOOLCHAIN_DIR);
        let isolated = isolated_toolchain_env(&root);
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
}
