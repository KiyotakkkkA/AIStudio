import { mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const REPO_ROOT = fileURLToPath(new URL("../..", import.meta.url));

const TEMP_ROOT = normalise(realpathSync(tmpdir()));

function normalise(path: string): string {
  const absolute = resolve(path);
  return process.platform === "win32" ? absolute.toLowerCase() : absolute;
}

/**
 * Fails loudly when a path a test is about to write to or delete is not inside the
 * operating system temp directory. Tests must never reach the real user-data folder.
 */
export function assertTemporaryPath(path: string): string {
  const candidate = normalise(path);
  if (candidate !== TEMP_ROOT && !candidate.startsWith(TEMP_ROOT + sep)) {
    throw new Error(`Refusing to use ${path}: tests may only write under ${tmpdir()}`);
  }
  return path;
}

export interface TemporaryDirectory {
  readonly path: string;
  dispose(): void;
}

const WINDOWS_LOCK_CODES = new Set(["EPERM", "EBUSY", "ENOTEMPTY"]);
const lockedOnDispose = new Set<string>();

function removeTree(path: string): void {
  rmSync(path, { recursive: true, force: true, maxRetries: 20, retryDelay: 50 });
}

function isWindowsLock(error: unknown): boolean {
  return (
    process.platform === "win32" &&
    error instanceof Error &&
    "code" in error &&
    typeof error.code === "string" &&
    WINDOWS_LOCK_CODES.has(error.code)
  );
}

process.once("exit", () => {
  for (const path of lockedOnDispose) {
    try {
      removeTree(path);
    } catch {
      continue;
    }
  }
});

export function temporaryDirectory(prefix = "studio-"): TemporaryDirectory {
  const path = assertTemporaryPath(mkdtempSync(join(realpathSync(tmpdir()), prefix)));
  return {
    path,
    dispose(): void {
      assertTemporaryPath(path);
      try {
        removeTree(path);
      } catch (error: unknown) {
        if (!isWindowsLock(error)) throw error;
        lockedOnDispose.add(path);
      }
    },
  };
}
