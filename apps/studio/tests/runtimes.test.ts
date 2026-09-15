import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { AppErrorCode, type DeviceProfileDto, type DownloadDto, type Timestamp } from "@zvs/shared";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import type { CatalogueItem } from "../src/host/downloads/catalogue.ts";
import type { DownloadEntity } from "../src/host/data/schema/index.ts";
import {
  definitionOf,
  formatOf,
  planAccelerator,
  RUNTIME_DEFINITIONS,
} from "../src/host/runtimes/definitions.ts";
import { ReleaseResolver } from "../src/host/runtimes/releases.ts";
import {
  parseRuntimeRef,
  RuntimeService,
  runtimeRef,
} from "../src/host/runtimes/RuntimeService.ts";
import type { LlamaServer } from "../src/host/runtimes/LlamaServer.ts";

const TAG = "b10970";

/** A trimmed copy of what the GitHub releases endpoint actually returns for llama.cpp. */
function releasePayload(): unknown {
  const asset = (name: string, size: number): unknown => ({
    name,
    size,
    state: "uploaded",
    browser_download_url: `https://github.test/${name}`,
    digest: `sha256:${"a".repeat(64)}`,
  });
  return [
    { tag_name: "v0.4.1", draft: false, prerelease: false, assets: [asset("nightly-tag.txt", 7)] },
    {
      tag_name: TAG,
      draft: false,
      prerelease: true,
      assets: [
        asset(`llama-${TAG}-bin-win-cpu-x64.zip`, 18_428_751),
        asset(`llama-${TAG}-bin-win-vulkan-x64.zip`, 31_675_940),
        asset(`llama-${TAG}-bin-win-cuda-13.3-x64.zip`, 149_722_826),
        asset("cudart-llama-bin-win-cuda-13.3-x64.zip", 390_970_417),
        asset(`llama-${TAG}-bin-ubuntu-vulkan-x64.tar.gz`, 30_188_817),
        asset(`llama-${TAG}-bin-macos-arm64.tar.gz`, 11_149_707),
      ],
    },
  ];
}

function resolverOver(
  payload: unknown,
  platform: NodeJS.Platform,
  arch = "x64",
): { resolver: ReleaseResolver; fetch: ReturnType<typeof vi.fn> } {
  const fetch = vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    json: () => Promise.resolve(payload),
  });
  return {
    resolver: new ReleaseResolver({ fetch: fetch as never, platform, arch }),
    fetch,
  };
}

function device(gpu: DeviceProfileDto["gpu"]): DeviceProfileDto {
  return {
    platform: "win32",
    arch: "x64",
    cpuModel: "Test CPU",
    cpuCores: 16,
    totalMemoryBytes: 32 * 1024 ** 3,
    freeMemoryBytes: 16 * 1024 ** 3,
    freeDiskBytes: 500 * 1024 ** 3,
    gpu,
    tier: "high",
    measuredAt: 1 as Timestamp,
  };
}

let workspace: TemporaryDirectory;
let settings: Map<string, unknown>;
let offered: CatalogueItem[];
let started: string[];
let downloads: DownloadDto[];

function service(overrides: Partial<Parameters<typeof makeService>[0]> = {}) {
  return makeService(overrides);
}

function makeService(options: {
  gpu?: DeviceProfileDto["gpu"];
  platform?: NodeJS.Platform;
  resolver?: ReleaseResolver;
  createServer?: (given: { modelPath: string; gpuLayers: number }) => LlamaServer;
}) {
  return new RuntimeService({
    downloads: {
      start: (input) => {
        started.push(input.ref);
        return Promise.resolve({ itemRef: input.ref } as DownloadDto);
      },
      list: () => downloads,
    },
    catalogue: {
      offer: (item) => {
        offered.push(item);
        return item;
      },
    },
    settings: {
      get: (key) =>
        settings.has(key)
          ? { key, value: settings.get(key) as never, updatedAt: 1 as Timestamp }
          : undefined,
      set: (key, value) => {
        settings.set(key, value);
        return { key, value: value as never, updatedAt: 1 as Timestamp };
      },
    },
    downloadsDir: workspace.path,
    device: { profile: () => Promise.resolve(device(options.gpu ?? null)) },
    platform: options.platform ?? "win32",
    arch: "x64",
    ...(options.resolver === undefined ? {} : { resolver: options.resolver }),
    ...(options.createServer === undefined ? {} : { createServer: options.createServer as never }),
  });
}

beforeEach(() => {
  workspace = temporaryDirectory("runtimes-");
  settings = new Map();
  offered = [];
  started = [];
  downloads = [];
});
afterEach(() => {
  workspace.dispose();
});

test("the recommended build follows what was detected, and says why", () => {
  expect(planAccelerator(device(null), "win32", "x64").accelerator).toBe("cpu");
  expect(
    planAccelerator(
      device({ vendor: "Intel", model: "UHD 770", vramBytes: null, discrete: false }),
      "win32",
      "x64",
    ).accelerator,
  ).toBe("cpu");

  const discrete = planAccelerator(
    device({ vendor: "NVIDIA", model: "RTX 4070", vramBytes: null, discrete: true }),
    "win32",
    "x64",
  );
  expect(discrete.accelerator).toBe("vulkan");
  expect(discrete.reason).toContain("RTX 4070");

  expect(planAccelerator(null, "darwin", "arm64").accelerator).toBe("metal");
});

test("only the builds that exist for a platform are offered", () => {
  const forWindows = RUNTIME_DEFINITIONS.filter(
    (definition) => definition.assets("win32", "x64") !== undefined,
  ).map((definition) => definition.id);
  const forMac = RUNTIME_DEFINITIONS.filter(
    (definition) => definition.assets("darwin", "arm64") !== undefined,
  ).map((definition) => definition.id);

  expect(forWindows).toEqual(["llama-cpp-cpu", "llama-cpp-vulkan", "llama-cpp-cuda"]);
  expect(forMac).toEqual(["llama-cpp-cpu", "llama-cpp-metal"]);
});

test("the resolver picks the newest build tag and skips the nightly pointer release", async () => {
  const { resolver } = resolverOver(releasePayload(), "win32");

  const build = await resolver.resolve(definitionOf("llama-cpp-vulkan")!);

  expect(build?.tag).toBe(TAG);
  expect(build?.assets).toHaveLength(1);
  expect(build?.assets[0]?.name).toBe(`llama-${TAG}-bin-win-vulkan-x64.zip`);
  expect(build?.assets[0]?.checksum?.algorithm).toBe("sha256");
});

test("a CUDA build brings its runtime libraries with it", async () => {
  const { resolver } = resolverOver(releasePayload(), "win32");

  const build = await resolver.resolve(definitionOf("llama-cpp-cuda")!);

  expect(build?.assets.map((asset) => asset.name)).toEqual([
    `llama-${TAG}-bin-win-cuda-13.3-x64.zip`,
    "cudart-llama-bin-win-cuda-13.3-x64.zip",
  ]);
});

test("a CUDA build whose runtime libraries are missing is not offered at all", async () => {
  const payload = releasePayload() as { assets: { name: string }[] }[];
  payload[1]!.assets = payload[1]!.assets.filter((asset) => !asset.name.startsWith("cudart-"));
  const { resolver } = resolverOver(payload, "win32");

  expect(await resolver.resolve(definitionOf("llama-cpp-cuda")!)).toBeUndefined();
});

test("a build that does not exist for this platform resolves to nothing, without a request", async () => {
  const { resolver, fetch } = resolverOver(releasePayload(), "darwin", "arm64");

  expect(await resolver.resolve(definitionOf("llama-cpp-cuda")!)).toBeUndefined();
  expect(fetch).not.toHaveBeenCalled();
});

test("the release list is read once and reused", async () => {
  const { resolver, fetch } = resolverOver(releasePayload(), "win32");

  await resolver.resolve(definitionOf("llama-cpp-cpu")!);
  await resolver.resolve(definitionOf("llama-cpp-vulkan")!);

  expect(fetch).toHaveBeenCalledTimes(1);
});

test("a rate-limited GitHub is reported as such rather than as a missing build", async () => {
  const fetch = vi.fn().mockResolvedValue({ ok: false, status: 403, json: () => ({}) });
  const resolver = new ReleaseResolver({ fetch: fetch as never, platform: "win32", arch: "x64" });

  await expect(resolver.resolve(definitionOf("llama-cpp-cpu")!)).rejects.toMatchObject({
    code: AppErrorCode.RATE_LIMITED,
  });
});

test("opening the page costs no network call, and marks the recommended build", async () => {
  const { resolver, fetch } = resolverOver(releasePayload(), "win32");
  const runtimes = service({
    gpu: { vendor: "NVIDIA", model: "RTX 4070", vramBytes: null, discrete: true },
    resolver,
  });

  const overview = await runtimes.overview();

  expect(fetch).not.toHaveBeenCalled();
  expect(overview.plan.accelerator).toBe("vulkan");
  expect(overview.plan.ready).toBe(false);
  expect(overview.runtimes.find((row) => row.recommended)?.id).toBe("llama-cpp-vulkan");
  expect(overview.runtimes.find((row) => row.id === "llama-cpp-metal")?.state).toBe("unsupported");
  expect(overview.recommendedModels).toContain("curated:embedding:bge-m3-f16");
});

test("installing with no id queues the archives of the recommended build", async () => {
  const { resolver } = resolverOver(releasePayload(), "win32");
  const runtimes = service({
    gpu: { vendor: "AMD", model: "RX 7800", vramBytes: null, discrete: true },
    resolver,
  });

  await runtimes.install();

  expect(offered).toHaveLength(1);
  expect(offered[0]?.kind).toBe("runtime");
  expect(offered[0]?.fileName).toBe(`llama-${TAG}-bin-win-vulkan-x64.zip`);
  expect(started).toEqual([
    runtimeRef("llama-cpp-vulkan", TAG, `llama-${TAG}-bin-win-vulkan-x64.zip`),
  ]);
});

test("an unpacked archive records the build tag and the server it found", async () => {
  const runtimes = service({});
  const archive = join(workspace.path, "runtimes", `llama-${TAG}-bin-win-vulkan-x64.zip`);
  mkdirSync(join(workspace.path, "runtimes"), { recursive: true });
  writeFileSync(archive, zipWith("llama-server.exe", "binary"));

  await runtimes.unpackDownload({
    itemKind: "runtime",
    itemRef: runtimeRef("llama-cpp-vulkan", TAG, `llama-${TAG}-bin-win-vulkan-x64.zip`),
    targetPath: archive,
  } as DownloadEntity);

  const overview = await runtimes.overview();
  const row = overview.runtimes.find((candidate) => candidate.id === "llama-cpp-vulkan");
  expect(row?.state).toBe("installed");
  expect(row?.installedVersion).toBe(TAG);
  expect(row?.executablePath).toContain("llama-server.exe");
  expect(overview.plan.ready).toBe(true);
});

test("a download of another kind is left alone by the unpacker", async () => {
  const runtimes = service({});

  await runtimes.unpackDownload({
    itemKind: "embedding",
    itemRef: "curated:embedding:bge-m3-f16",
    targetPath: join(workspace.path, "embeddings", "bge-m3-FP16.gguf"),
  } as DownloadEntity);

  expect(settings.size).toBe(0);
});

test("embedding without an installed engine explains what to do about it", async () => {
  const runtimes = service({});

  await expect(
    runtimes.embed("bge-m3-FP16.gguf", ["привет"], AbortSignal.timeout(1_000)),
  ).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
});

test("an .onnx file is refused with its own reason, not the missing-engine one", async () => {
  const runtimes = service({});

  await expect(
    runtimes.embed("bge-m3.onnx", ["привет"], AbortSignal.timeout(1_000)),
  ).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
  expect(formatOf("bge-m3.onnx")).toBe("onnx");
  expect(formatOf("bge-m3-FP16.gguf")).toBe("gguf");
  expect(formatOf("notes.txt")).toBeUndefined();
});

test("an accelerated build is started with layers on the GPU, a CPU build with none", async () => {
  const spawned: { modelPath: string; gpuLayers: number }[] = [];
  const fake = (given: { modelPath: string; gpuLayers: number }): LlamaServer => {
    spawned.push(given);
    return {
      running: true,
      modelPath: given.modelPath,
      embed: () => Promise.resolve([Float32Array.from([0.5, 0.25])]),
      start: () => Promise.resolve("http://127.0.0.1:1"),
      stop: () => undefined,
    } as unknown as LlamaServer;
  };
  mkdirSync(join(workspace.path, "embeddings"), { recursive: true });
  writeFileSync(join(workspace.path, "embeddings", "bge-m3-FP16.gguf"), "weights");
  settings.set("runtimes.installed", {
    "llama-cpp-vulkan": {
      tag: TAG,
      installPath: join(workspace.path, "runtimes", "llama-cpp-vulkan"),
      executablePath: join(workspace.path, "runtimes", "llama-cpp-vulkan", "llama-server.exe"),
      sizeBytes: 10,
    },
  });
  const runtimes = service({ createServer: fake });

  const vectors = await runtimes.embed("bge-m3-FP16.gguf", ["привет"], AbortSignal.timeout(1_000));

  expect(vectors[0]).toEqual(Float32Array.from([0.5, 0.25]));
  expect(spawned).toHaveLength(1);
  expect(spawned[0]?.gpuLayers).toBe(999);
  expect(spawned[0]?.modelPath).toContain("bge-m3-FP16.gguf");
});

test("a runtime ref survives a round trip and rejects anything else", () => {
  const ref = runtimeRef("llama-cpp-cuda", TAG, "cudart-llama-bin-win-cuda-13.3-x64.zip");

  expect(parseRuntimeRef(ref)).toEqual({
    runtimeId: "llama-cpp-cuda",
    tag: TAG,
    assetName: "cudart-llama-bin-win-cuda-13.3-x64.zip",
  });
  expect(parseRuntimeRef("curated:embedding:bge-m3-f16")).toBeUndefined();
});

/** One stored member, which is enough for the unpacker to find an executable. */
function zipWith(name: string, body: string): Buffer {
  const nameBytes = Buffer.from(name, "utf8");
  const content = Buffer.from(body, "utf8");
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt32LE(content.length, 18);
  local.writeUInt32LE(content.length, 22);
  local.writeUInt16LE(nameBytes.length, 26);
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt32LE(content.length, 20);
  central.writeUInt32LE(content.length, 24);
  central.writeUInt16LE(nameBytes.length, 28);
  central.writeUInt32LE(0, 42);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(1, 8);
  eocd.writeUInt16LE(1, 10);
  eocd.writeUInt32LE(central.length + nameBytes.length, 12);
  eocd.writeUInt32LE(local.length + nameBytes.length + content.length, 16);
  return Buffer.concat([local, nameBytes, content, central, nameBytes, eocd]);
}
