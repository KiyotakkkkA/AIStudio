import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const from = (relative: string): string => fileURLToPath(new URL(relative, import.meta.url));

const alias = {
  "@zvs/shared": from("./packages/shared/src/index.ts"),
  "@zvs/ipc": from("./packages/ipc/src/index.ts"),
};

const shared = {
  resolve: { alias },
  esbuild: { jsx: "automatic" as const },
};

export default defineConfig({
  test: {
    projects: [
      {
        ...shared,
        test: {
          name: "packages",
          environment: "node",
          include: ["packages/*/tests/**/*.test.{ts,js}"],
          testTimeout: 60_000,
        },
      },
      {
        ...shared,
        test: {
          name: "host",
          environment: "node",
          include: ["apps/studio/tests/*.test.ts"],
        },
      },
      {
        ...shared,
        test: {
          name: "renderer",
          environment: "jsdom",
          include: ["apps/studio/tests/renderer/**/*.test.{ts,tsx}"],
          setupFiles: [from("./apps/studio/tests/renderer/setup.ts")],
        },
      },
    ],
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "html", "lcov"],
      reportsDirectory: "coverage",
      include: ["packages/*/src/**/*.ts", "apps/studio/src/**/*.{ts,tsx}"],
      exclude: ["**/*.d.ts", "apps/studio/src/renderer/main.tsx", "apps/studio/src/host/main.ts"],
    },
  },
});
