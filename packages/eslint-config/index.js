import path from "node:path";
import js from "@eslint/js";
import importPlugin from "eslint-plugin-import";
import tseslint from "typescript-eslint";

export function createConfig(root) {
  const zones = [
    // Shared DTOs must stay independent of every application domain.
    { target: "./packages/shared", from: "./apps" },
    // Renderer code must never gain access to privileged host implementation.
    { target: "./apps/studio/src/renderer", from: "./apps/studio/src/host" },
    // Host logic must stay independent of presentation code.
    { target: "./apps/studio/src/host", from: "./apps/studio/src/renderer" },
    // Reusable UI must not depend on domain features or application state.
    {
      target: "./apps/studio/src/renderer/ui",
      from: ["./apps/studio/src/renderer/features", "./apps/studio/src/renderer/stores"],
    },
  ];
  const clientZone = {
    target: [
      "./packages",
      "./apps/studio/src/preload",
      "./apps/studio/src/browser-ui",
      "./apps/studio/src/renderer",
      "./apps/studio/src/host/browser",
      "./apps/studio/src/host/documents",
      "./apps/studio/src/host/drivers",
      "./apps/studio/src/host/ipc",
      "./apps/studio/src/host/kernel",
      "./apps/studio/src/host/platform",
      "./apps/studio/src/host/services",
    ],
    from: "./apps/studio/src/host/data/client.ts",
  };
  const drizzleImports = [
    "error",
    {
      paths: ["better-sqlite3"],
      patterns: ["drizzle-orm", "drizzle-orm/*", "drizzle-orm/**"],
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
        "import/no-restricted-paths": restrictedPaths([...zones, clientZone]),
      },
    },
    {
      files: ["apps/studio/src/**/*.{ts,tsx}"],
      rules: { "no-restricted-imports": drizzleImports },
    },
    {
      files: ["apps/studio/src/host/data/**/*.{ts,tsx}"],
      rules: { "no-restricted-imports": "off" },
    },
  ];
}

export default createConfig(path.resolve(import.meta.dirname, "../.."));
