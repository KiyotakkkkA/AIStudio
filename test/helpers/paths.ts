import { mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const REPO_ROOT = fileURLToPath(new URL("../..", import.meta.url));
export const STUDIO_ROOT = join(REPO_ROOT, "apps", "studio");
export const MIGRATIONS_DIR = join(STUDIO_ROOT, "src", "host", "data", "migrations");

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

export function temporaryDirectory(prefix = "studio-"): TemporaryDirectory {
  const path = assertTemporaryPath(mkdtempSync(join(realpathSync(tmpdir()), prefix)));
  return {
    path,
    dispose(): void {
      assertTemporaryPath(path);
      rmSync(path, { recursive: true, force: true, maxRetries: 10, retryDelay: 50 });
    },
  };
}
