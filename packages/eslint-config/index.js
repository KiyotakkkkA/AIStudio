import path from "node:path";
import js from "@eslint/js";
import importPlugin from "eslint-plugin-import";
import tseslint from "typescript-eslint";

export function createConfig(root) {
  const zones = [
    // Shared DTOs must stay independent of every application domain.
    { target: "./packages/shared", from: "./apps" },
    // Reusable UI must not depend on domain features or application state.
    {
      target: "./apps/studio/src/renderer/ui",
      from: ["./apps/studio/src/renderer/features", "./apps/studio/src/renderer/stores"],
    },
  ];
  const restrictedPaths = (rules) => ["error", { basePath: root, zones: rules }];

  return [
    {
      ignores: [
        "**/node_modules/**",
        "**/dist/**",
        "**/out/**",
        "**/build/**",
        "**/target/**",
        "**/src-tauri/gen/**",
        "**/.turbo/**",
        "**/coverage/**",
        "design/**",
        "references/**",
        "tasks/**",
      ],
    },
    js.configs.recommended,
    ...tseslint.configs.recommended,
    {
      files: ["**/*.{js,mjs,cjs,ts,tsx,mts,cts}"],
      plugins: { import: importPlugin },
      settings: {
        "import/resolver": {
          typescript: {
            project: [
              path.join(root, "apps/*/tsconfig*.json"),
              path.join(root, "packages/*/tsconfig.json"),
            ],
          },
        },
      },
      rules: {
        "import/no-unresolved": "error",
        "import/no-restricted-paths": restrictedPaths(zones),
      },
    },
    {
      files: ["crates/*/skills/*/assets/**/*.js"],
      languageOptions: {
        sourceType: "script",
        globals: {
          window: "readonly",
          document: "readonly",
          console: "readonly",
          Option: "readonly",
          ResizeObserver: "readonly",
          requestAnimationFrame: "readonly",
          cancelAnimationFrame: "readonly",
        },
      },
    },
  ];
}

export default createConfig(path.resolve(import.meta.dirname, "../.."));
