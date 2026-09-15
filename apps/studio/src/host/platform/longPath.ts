import { isAbsolute } from "node:path";

const EXTENDED = "\\\\?\\";
const EXTENDED_UNC = "\\\\?\\UNC\\";

/**
 * Windows refuses a path longer than 260 characters through the ordinary Win32 layer, whatever
 * the filesystem can hold. A corpus of scanned case files reaches that easily — a few nested
 * folders with descriptive Russian names and a long scan name is enough — and the failure is
 * silent in the worst way: `readdir` and `stat` raise the same errors they raise for a missing or
 * unreadable path, so a whole subtree reads as "недоступна" and the index quietly comes up short.
 *
 * The `\\?\` prefix opts a single call out of that limit. It is applied at the filesystem
 * boundary and nowhere else: the prefix disables path normalisation, so anything carrying it must
 * already be absolute and clean, and it must never reach a database row, a DTO or the screen.
 */
export function toExtendedPath(path: string, platform: NodeJS.Platform = process.platform): string {
  if (platform !== "win32") return path;
  if (path.startsWith(EXTENDED)) return path;
  if (!isAbsolute(path)) return path;

  const backslashed = path.replaceAll("/", "\\");
  // `\\server\share` is a UNC path, which takes its own spelling of the prefix.
  if (backslashed.startsWith("\\\\")) return `${EXTENDED_UNC}${backslashed.slice(2)}`;
  return `${EXTENDED}${backslashed}`;
}

/** Strips the prefix, for a path on its way into storage or onto the screen. */
export function fromExtendedPath(path: string): string {
  if (path.startsWith(EXTENDED_UNC)) return `\\\\${path.slice(EXTENDED_UNC.length)}`;
  if (path.startsWith(EXTENDED)) return path.slice(EXTENDED.length);
  return path;
}
