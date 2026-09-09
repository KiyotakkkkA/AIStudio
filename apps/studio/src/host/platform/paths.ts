import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export interface PathEnvironment {
  userData: string;
  resources: string;
  appRoot: string;
  packaged: boolean;
}

export const userDataDir = (env: PathEnvironment): string => env.userData;
export const dbPath = (env: PathEnvironment): string => join(userDataDir(env), "studio.sqlite");
export const logsDir = (env: PathEnvironment): string => join(userDataDir(env), "logs");
export const cacheDir = (env: PathEnvironment): string => join(userDataDir(env), "cache");
export const streamsDir = (env: PathEnvironment): string => join(logsDir(env), "streams");
export const backupsDir = (env: PathEnvironment): string => join(userDataDir(env), "backups");
export const resourcesDir = (env: PathEnvironment): string =>
  env.packaged ? env.resources : join(env.appRoot, "resources");

export function resolvePaths(env: PathEnvironment) {
  const hostDir = dirname(fileURLToPath(import.meta.url));
  return {
    userDataDir: userDataDir(env),
    dbPath: dbPath(env),
    logsDir: logsDir(env),
    streamsDir: streamsDir(env),
    backupsDir: backupsDir(env),
    cacheDir: cacheDir(env),
    resourcesDir: resourcesDir(env),
    migrationsDir: join(hostDir, "migrations"),
    preloadPath: join(hostDir, "../preload/index.cjs"),
    browserPreloadPath: join(hostDir, "../preload/browser.cjs"),
    sitePreloadPath: join(hostDir, "../preload/site.cjs"),
    browserRendererUrl: pathToFileURL(join(hostDir, "../renderer/browser-ui/index.html")).href,
    rendererUrl: pathToFileURL(join(hostDir, "../renderer/index.html")).href,
  };
}

export function logFilePath(directory: string, rotation = 0): string {
  return join(directory, rotation === 0 ? "studio.jsonl" : `studio.${rotation}.jsonl`);
}

export function streamFilePath(directory: string, at: number): string {
  return join(directory, `${new Date(at).toISOString().slice(0, 10)}.jsonl`);
}

export type StudioPaths = ReturnType<typeof resolvePaths>;
