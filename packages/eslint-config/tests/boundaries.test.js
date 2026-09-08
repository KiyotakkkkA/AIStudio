import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { ESLint } from "eslint";
import { createConfig } from "../index.js";

test("architecture boundaries resolve paths and reject forbidden imports", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "zvs-boundaries-"));
  const host = "apps/studio/src/host";
  const renderer = "apps/studio/src/renderer";
  const shared = "packages/shared/src/index.ts";
  const files = [
    shared,
    `${host}/entry.ts`,
    `${host}/data/client.ts`,
    `${host}/data/migrate.ts`,
    `${host}/main.ts`,
    `${renderer}/entry.ts`,
    `${renderer}/features/example.ts`,
    `${renderer}/stores/example.ts`,
  ];
  try {
    for (const file of files) {
      await mkdir(path.dirname(path.join(root, file)), { recursive: true });
      await writeFile(path.join(root, file), "export const value = 1;\n");
    }
    await writeFile(
      path.join(root, "apps/studio/tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          paths: { "@host/*": ["./src/host/*"], "@renderer/*": ["./src/renderer/*"] },
        },
      }),
    );
    const eslint = new ESLint({
      cwd: root,
      overrideConfigFile: true,
      overrideConfig: createConfig(root),
    });
    const cases = [
      [shared, "../../../apps/studio/src/host/entry", true],
      [shared, "../../../apps/studio/src/renderer/entry", true],
      [`${renderer}/check.ts`, "../host/entry", true],
      [`${host}/check.ts`, "../renderer/entry", true],
      [`${renderer}/check.ts`, "@host/entry", true],
      [`${host}/check.ts`, "@renderer/entry", true],
      [`${renderer}/ui/atoms/check.ts`, "../../features/example", true],
      [`${renderer}/ui/atoms/check.ts`, "../../stores/example", true],
      [`${host}/services/check.ts`, "../data/client", true],
      [`${host}/data/repositories/check.ts`, "../client", false],
      [`${host}/data/migrate.ts`, "./client", false],
      [`${host}/data/other.ts`, "./client", false],
      [`${host}/main.ts`, "./data/client", false],
      [`${host}/ipc/check.ts`, "../data/client", true],
      [`${host}/drivers/check.ts`, "../data/client", true],
      [`${host}/kernel/check.ts`, "../data/client", true],
      [`${renderer}/check.ts`, "../host/data/client", true],
      [`${host}/check.ts`, "../../../../packages/shared/src/index", false],
      [`${renderer}/check.ts`, "../../../../packages/shared/src/index", false],
      [`${renderer}/features/check.ts`, "../stores/example", false],
    ];
    for (const [file, source, forbidden] of cases) {
      const [result] = await eslint.lintText(`export { value } from "${source}";\n`, {
        filePath: path.join(root, file),
      });
      assert.equal(
        result.messages.some((message) => message.ruleId === "import/no-unresolved"),
        false,
        `${file}: ${source} resolves`,
      );
      assert.equal(
        result.messages.some((message) => message.ruleId === "import/no-restricted-paths"),
        forbidden,
        `${file}: ${source}`,
      );
      if (!forbidden) assert.equal(result.errorCount, 0, JSON.stringify(result.messages));
    }

    const drizzleCases = [
      [`${host}/services/check.ts`, "drizzle-orm", true],
      [`${host}/ipc/check.ts`, "better-sqlite3", true],
      [`${renderer}/check.ts`, "drizzle-orm/sqlite-core", true],
      [`${host}/data/repositories/check.ts`, "drizzle-orm", false],
      [`${host}/data/client.ts`, "better-sqlite3", false],
    ];
    for (const [file, source, forbidden] of drizzleCases) {
      const [result] = await eslint.lintText(
        `import "${source}";
`,
        {
          filePath: path.join(root, file),
        },
      );
      assert.equal(
        result.messages.some((message) => message.ruleId === "no-restricted-imports"),
        forbidden,
        `${file}: ${source}`,
      );
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
