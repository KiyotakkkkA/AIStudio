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
export const vectorStoresDir = (env: PathEnvironment): string => join(userDataDir(env), "vectors");
export function vectorStorePath(directory: string, storeId: string): string {
  if (!/^[a-zA-Z0-9_-]+$/.test(storeId)) throw new Error("Invalid vector store ID");
  return join(directory, storeId);
}
export const downloadsDir = (env: PathEnvironment): string => join(userDataDir(env), "downloads");

/** Where a finished download lands, one directory per catalogue kind. */
export const DOWNLOAD_DIRECTORIES = {
  model: "models",
  embedding: "embeddings",
  mcp: "mcp",
  skill: "skills",
} as const;

export function downloadTargetPath(directory: string, kind: string, fileName: string): string {
  const folder = DOWNLOAD_DIRECTORIES[kind as keyof typeof DOWNLOAD_DIRECTORIES];
  if (folder === undefined) throw new Error(`Unknown download kind: ${kind}`);
  if (!/^[a-zA-Z0-9._-]+$/.test(fileName) || fileName === "." || fileName === "..")
    throw new Error(`Unsafe download file name: ${fileName}`);
  return join(directory, folder, fileName);
}

export const streamsDir = (env: PathEnvironment): string => join(logsDir(env), "streams");
export const backupsDir = (env: PathEnvironment): string => join(userDataDir(env), "backups");
export const resourcesDir = (env: PathEnvironment): string =>
  env.packaged ? env.resources : join(env.appRoot, "resources");

export function nativeTargetTriple(platform = process.platform, arch = process.arch): string {
  const architecture = arch === "x64" ? "x86_64" : arch === "arm64" ? "aarch64" : undefined;
  if (!architecture) throw new Error(`Unsupported native architecture: ${arch}`);
  if (platform === "win32") return `${architecture}-pc-windows-msvc`;
  if (platform === "darwin") return `${architecture}-apple-darwin`;
  if (platform === "linux") return `${architecture}-unknown-linux-gnu`;
  throw new Error(`Unsupported native platform: ${platform}`);
}

export const nativeAddonPath = (env: PathEnvironment): string =>
  join(resourcesDir(env), "native", nativeTargetTriple(), "zvs-core.node");

export const sidecarPath = (env: PathEnvironment, platform = process.platform): string =>
  join(
    resourcesDir(env),
    "sidecar",
    nativeTargetTriple(platform),
    platform === "win32" ? "zvs-jobd.exe" : "zvs-jobd",
  );

export function resolvePaths(env: PathEnvironment) {
  const hostDir = dirname(fileURLToPath(import.meta.url));
  return {
    userDataDir: userDataDir(env),
    dbPath: dbPath(env),
    logsDir: logsDir(env),
    streamsDir: streamsDir(env),
    backupsDir: backupsDir(env),
    cacheDir: cacheDir(env),
    vectorStoresDir: vectorStoresDir(env),
    downloadsDir: downloadsDir(env),
    resourcesDir: resourcesDir(env),
    nativeAddonPath: nativeAddonPath(env),
    sidecarPath: sidecarPath(env),
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
