import { existsSync, statSync } from "node:fs";
import { delimiter as pathDelimiter, extname, isAbsolute, join } from "node:path";

export interface WindowsLaunch {
  readonly file: string;
  readonly args: readonly string[];
  readonly verbatim: boolean;
}

const META = /([()\][%!^"`<>&|;, *?])/g;
const DEFAULT_PATHEXT = ".COM;.EXE;.BAT;.CMD";

function envValue(env: NodeJS.ProcessEnv, name: string): string | undefined {
  const key = Object.keys(env).find((entry) => entry.toUpperCase() === name);
  return key === undefined ? undefined : env[key];
}

function isFile(path: string): boolean {
  try {
    return existsSync(path) && statSync(path).isFile();
  } catch {
    return false;
  }
}

export function resolveWindowsCommand(
  command: string,
  env: NodeJS.ProcessEnv,
  exists: (path: string) => boolean = isFile,
  cwd: string = process.cwd(),
): string | null {
  const extensions = (envValue(env, "PATHEXT") ?? DEFAULT_PATHEXT)
    .split(";")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
  const candidates = (base: string): string[] =>
    extname(base).length > 0 &&
    extensions.some((ext) => ext.toUpperCase() === extname(base).toUpperCase())
      ? [base]
      : [...extensions.map((ext) => `${base}${ext.toLowerCase()}`), base];

  if (/[\\/]/.test(command) || isAbsolute(command)) {
    const base = isAbsolute(command) ? command : join(cwd, command);
    return candidates(base).find((path) => exists(path)) ?? null;
  }
  const directories = (envValue(env, "PATH") ?? "")
    .split(pathDelimiter)
    .map((entry) => entry.trim().replace(/^"(.*)"$/, "$1"))
    .filter((entry) => entry.length > 0);
  for (const directory of [cwd, ...directories]) {
    const found = candidates(join(directory, command)).find((path) => exists(path));
    if (found !== undefined) return found;
  }
  return null;
}

function escapeArgument(argument: string): string {
  const quoted = argument.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\*)$/, "$1$1");
  return `"${quoted}"`.replace(META, "^$1");
}

export function windowsLaunch(
  command: string,
  args: readonly string[],
  env: NodeJS.ProcessEnv,
  exists?: (path: string) => boolean,
  cwd?: string,
): WindowsLaunch {
  const resolved = resolveWindowsCommand(command, env, exists, cwd) ?? command;
  if (!/\.(cmd|bat)$/i.test(resolved)) return { file: resolved, args, verbatim: false };
  const line = [resolved.replace(META, "^$1"), ...args.map(escapeArgument)].join(" ");
  return {
    file: envValue(env, "COMSPEC") ?? "cmd.exe",
    args: ["/d", "/s", "/c", `"${line}"`],
    verbatim: true,
  };
}
