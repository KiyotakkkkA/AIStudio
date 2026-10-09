import { join } from "node:path";

export function isolatedToolchainEnv(root: string): Record<string, string> {
  return {
    npm_config_cache: join(root, "npm-cache"),
    npm_config_prefix: join(root, "npm-global"),
    npm_config_update_notifier: "false",
    npm_config_fund: "false",
    UV_CACHE_DIR: join(root, "uv", "cache"),
    UV_TOOL_DIR: join(root, "uv", "tools"),
    UV_TOOL_BIN_DIR: join(root, "uv", "bin"),
    UV_PYTHON_INSTALL_DIR: join(root, "uv", "python"),
    PIP_CACHE_DIR: join(root, "pip-cache"),
  };
}
