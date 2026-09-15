import { mkdirSync, writeFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import { fromExtendedPath, toExtendedPath } from "../src/host/platform/longPath.ts";
import { walkSources } from "../src/host/indexing/walk.ts";
import type { VectorSourceEntity } from "../src/host/data/schema/index.ts";

let workspace: TemporaryDirectory;

beforeEach(() => {
  workspace = temporaryDirectory("longpath-");
});
afterEach(() => {
  workspace.dispose();
});

test("a drive path takes the prefix and a UNC path takes its own spelling", () => {
  expect(toExtendedPath("C:\\docs\\a.txt", "win32")).toBe("\\\\?\\C:\\docs\\a.txt");
  expect(toExtendedPath("C:/docs/a.txt", "win32")).toBe("\\\\?\\C:\\docs\\a.txt");
  expect(toExtendedPath("\\\\server\\share\\a.txt", "win32")).toBe(
    "\\\\?\\UNC\\server\\share\\a.txt",
  );
});

test("nothing is prefixed twice, relatively, or off Windows", () => {
  expect(toExtendedPath("\\\\?\\C:\\a.txt", "win32")).toBe("\\\\?\\C:\\a.txt");
  expect(toExtendedPath("docs\\a.txt", "win32")).toBe("docs\\a.txt");
  expect(toExtendedPath("/home/kirza/a.txt", "linux")).toBe("/home/kirza/a.txt");
});

test("the prefix round-trips off, for a path on its way to storage or the screen", () => {
  expect(fromExtendedPath(toExtendedPath("C:\\docs\\a.txt", "win32"))).toBe("C:\\docs\\a.txt");
  expect(fromExtendedPath(toExtendedPath("\\\\server\\share\\a.txt", "win32"))).toBe(
    "\\\\server\\share\\a.txt",
  );
  expect(fromExtendedPath("C:\\docs\\a.txt")).toBe("C:\\docs\\a.txt");
});

/** Nested folders named the way a case file tree is, until the path is past Windows' limit. */
function deepDirectory(root: string): string {
  let path = root;
  while (path.length < 300) path = join(path, "взыскание денежных средств (ИП Гусаров)");
  mkdirSync(toExtendedPath(path), { recursive: true });
  return path;
}

function folderSource(path: string): VectorSourceEntity {
  return {
    id: "0199dd11-1111-7111-8111-0000000000f1",
    storeId: "0199dd11-1111-7111-8111-0000000000f2",
    kind: "folder",
    path,
    include: [],
    exclude: [],
    recursive: true,
    createdAt: 1,
  } as VectorSourceEntity;
}

test.runIf(process.platform === "win32")(
  "a file past the 260-character limit is walked and read, not reported as unavailable",
  async () => {
    const deep = deepDirectory(workspace.path);
    const file = join(deep, "определение суда от 14.03.2026.txt");
    expect(file.length).toBeGreaterThan(260);
    writeFileSync(toExtendedPath(file), "спорная сумма взыскана", "utf8");

    const walked = await walkSources([folderSource(workspace.path)]);

    expect(walked.notes).toEqual([]);
    expect(walked.files.map((found) => found.path)).toEqual([file]);
    // The discovered path is the real one, with no prefix leaking into what gets stored.
    expect(walked.files[0]?.path.startsWith("\\\\?\\")).toBe(false);
    expect(await readFile(toExtendedPath(file), "utf8")).toBe("спорная сумма взыскана");
  },
);

/**
 * Whether an unprefixed long path works at all depends on the machine: Windows 10 1607+ lifts the
 * limit when `LongPathsEnabled` is set in the registry *and* the running binary declares itself
 * long-path aware. The development `node` binary on this machine does both, which is exactly why
 * the failure is so easy to miss in development and so easy to hit on a user's machine. The
 * prefix removes the dependency on either, so the test asserts that it works — never that the
 * alternative fails.
 */
test.runIf(process.platform === "win32")(
  "the prefix reads the same bytes back whether or not the machine needs it",
  async () => {
    const deep = deepDirectory(workspace.path);
    const file = join(deep, "определение суда от 14.03.2026.txt");
    writeFileSync(toExtendedPath(file), "спорная сумма взыскана", "utf8");

    expect(await readFile(toExtendedPath(file), "utf8")).toBe("спорная сумма взыскана");
    expect(toExtendedPath(file)).toContain("\\\\?\\");
  },
);
