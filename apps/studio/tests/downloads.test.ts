import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";
import { AppError, AppErrorCode, type DownloadDto } from "@zvs/shared";
import { temporaryDatabase, type TemporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import { createFakeClock, type FakeClock } from "../../../test/helpers/fakeClock.ts";
import { FakeDownloadJobs, sha256 } from "../../../test/helpers/FakeDownloadJobs.ts";
import {
  CatalogueService,
  type CatalogueItem,
  type CatalogueProvider,
  type InstalledItem,
} from "../src/host/downloads/catalogue.ts";
import { DiskService, directorySize, type DiskProbe } from "../src/host/downloads/disk.ts";
import { DownloadService, FREE_SPACE_MARGIN_BYTES } from "../src/host/downloads/DownloadService.ts";
import { createDownloadHandlers } from "../src/host/ipc/downloads.ts";
import { registerDownloadNodes } from "../src/host/kernel/downloadNodes.ts";
import { NodeRegistry } from "../src/host/kernel/NodeRegistry.ts";
import { RunService } from "../src/host/services/RunService.ts";
import { createEventBus } from "../src/host/platform/events.ts";

const GIB = 1024 ** 3;
const BODY = "0123456789abcdefghijklmnopqrstuvwxyz";

let db: TemporaryDatabase;
let workspace: TemporaryDirectory;
let downloadsDir: string;
let vectorStoresDir: string;
let clock: FakeClock;
let jobs: FakeDownloadJobs;
let disk: DiskService;
let downloads: DownloadService;
let runs: RunService;
let freeBytes: number;
let items: CatalogueItem[];
let present: InstalledItem[];

function item(
  overrides: Partial<CatalogueItem> & Pick<CatalogueItem, "ref" | "name">,
): CatalogueItem {
  return {
    kind: "model",
    source: "curated",
    displayName: overrides.name,
    description: "",
    sizeBytes: BODY.length,
    url: `https://example.test/${overrides.name}`,
    fileName: `${overrides.name}.bin`,
    tags: [],
    ...overrides,
  };
}

function targetOf(entry: CatalogueItem): string {
  const folder = { model: "models", embedding: "embeddings", mcp: "mcp", skill: "skills" }[
    entry.kind
  ];
  return join(downloadsDir, folder, entry.fileName);
}

/** Polls until the download stops moving. The queue drives runs, so there is nothing to await. */
async function settle(id: string): Promise<DownloadDto> {
  for (let attempt = 0; attempt < 600; attempt += 1) {
    const row = downloads.get(id);
    if (row.status !== "queued" && row.status !== "running") return row;
    await new Promise((resume) => setTimeout(resume, 5));
  }
  throw new Error(`Download never settled: ${downloads.get(id).status}`);
}

/** Builds the pair, which is circular: the kernel executes downloads, downloads start runs. */
function build(concurrency: number): void {
  const probe: DiskProbe = {
    free: () => Promise.resolve({ totalBytes: 1000 * GIB, freeBytes }),
    size: (path) => directorySize(path),
  };
  const provider: CatalogueProvider = {
    id: "curated",
    live: false,
    list: () => Promise.resolve(items),
    installed: () => Promise.resolve(present),
  };
  disk = new DiskService({
    downloadsDir,
    vectorStoresDir,
    userDataDir: workspace.path,
    probe,
    ttlMs: 0,
    clock,
  });
  downloads = new DownloadService({
    data: db.client,
    jobs,
    disk,
    catalogue: new CatalogueService({ providers: [provider], ttlMs: 0, clock }),
    downloadsDir,
    clock,
    concurrency,
    persistIntervalMs: 0,
  });
  runs = new RunService({
    data: db.client,
    events: createEventBus(),
    registry: registerDownloadNodes(new NodeRegistry()),
    services: { downloads },
    clock,
  });
  downloads.attach(runs);
}

beforeEach(() => {
  db = temporaryDatabase();
  workspace = temporaryDirectory("studio-downloads-");
  downloadsDir = join(workspace.path, "downloads");
  vectorStoresDir = join(workspace.path, "vectors");
  mkdirSync(vectorStoresDir, { recursive: true });
  clock = createFakeClock();
  jobs = new FakeDownloadJobs();
  freeBytes = 100 * GIB;
  items = [item({ ref: "curated:model:alpha", name: "alpha" })];
  present = [];
  build(2);
});

afterEach(async () => {
  downloads.dispose();
  await runs.dispose();
  db.dispose();
  workspace.dispose();
});

test("a download lands at its target and shows up as a run on the Tasks page", async () => {
  jobs.serve("https://example.test/alpha", BODY);
  // A free slot is claimed synchronously, so the row is already running when start returns.
  const started = await downloads.start({ ref: "curated:model:alpha" });
  expect(started.status).toBe("running");

  const finished = await settle(started.id);
  expect(finished.status).toBe("succeeded");
  expect(finished.bytesDone).toBe(BODY.length);
  expect(readFileSync(targetOf(items[0]!), "utf8")).toBe(BODY);
  expect(existsSync(`${finished.targetPath}.part`)).toBe(false);

  // One queue, two views: the same work is a run like any other.
  const page = runs.list({ live: false, limit: 50 });
  expect(page.items).toHaveLength(1);
  expect(page.items[0]?.kind).toBe("download");
  expect(page.items[0]?.subjectId).toBe(started.id);
  expect(page.counts.byKind.download).toBe(1);
});

test("a partial file resumes from where it stopped instead of restarting", async () => {
  jobs.serve("https://example.test/alpha", BODY);
  const target = targetOf(items[0]!);
  mkdirSync(join(target, ".."), { recursive: true });
  writeFileSync(`${target}.part`, BODY.slice(0, 12));

  const started = await downloads.start({ ref: "curated:model:alpha" });
  // The byte count is seeded from the partial, so the page does not flash back to zero.
  expect(started.bytesDone).toBe(12);
  expect((await settle(started.id)).status).toBe("succeeded");

  expect(jobs.attempts).toHaveLength(1);
  expect(jobs.attempts[0]?.resumedFrom).toBe(12);
  expect(readFileSync(target, "utf8")).toBe(BODY);
});

test("a checksum mismatch fails the download and installs nothing", async () => {
  items = [
    item({
      ref: "curated:model:alpha",
      name: "alpha",
      checksum: { algorithm: "sha256", value: sha256("something else") },
    }),
  ];
  jobs.serve("https://example.test/alpha", BODY);

  const started = await downloads.start({ ref: "curated:model:alpha" });
  const failed = await settle(started.id);

  expect(failed.status).toBe("failed");
  expect(failed.error).toMatch(/Checksum mismatch/);
  expect(existsSync(targetOf(items[0]!))).toBe(false);
  expect(existsSync(`${failed.targetPath}.part`)).toBe(false);
});

test("cancelling deletes the partial file and pausing keeps it", async () => {
  jobs.serve("https://example.test/alpha", BODY, 6);
  jobs.hold("https://example.test/alpha");
  const started = await downloads.start({ ref: "curated:model:alpha" });
  await jobs.started(1);

  const pausing = downloads.pause(started.id);
  jobs.release("https://example.test/alpha");
  expect((await pausing).status).toBe("paused");
  expect(existsSync(`${started.targetPath}.part`)).toBe(true);

  // Resuming picks the partial up again rather than starting over.
  downloads.resume(started.id);
  expect((await settle(started.id)).status).toBe("succeeded");
  expect(jobs.attempts).toHaveLength(2);
  expect(jobs.attempts[1]?.resumedFrom).toBeGreaterThan(0);

  jobs.serve("https://example.test/beta", BODY, 6);
  items.push(item({ ref: "curated:model:beta", name: "beta" }));
  jobs.hold("https://example.test/beta");
  const second = await downloads.start({ ref: "curated:model:beta" });
  await jobs.started(3);
  const cancelling = downloads.cancel(second.id);
  jobs.release("https://example.test/beta");

  expect((await cancelling).status).toBe("cancelled");
  expect(existsSync(`${second.targetPath}.part`)).toBe(false);
  expect(existsSync(second.targetPath)).toBe(false);
});

test("the queue respects the concurrency cap and lets a small package overtake a big model", async () => {
  build(1);
  freeBytes = 500 * GIB;
  items = [
    item({ ref: "curated:model:alpha", name: "alpha" }),
    item({ ref: "curated:model:big", name: "big", sizeBytes: 240 * GIB }),
    item({ ref: "curated:mcp:small", name: "small", kind: "mcp", sizeBytes: 1024 }),
  ];
  for (const entry of items) jobs.serve(entry.url, BODY, 6);
  jobs.hold("https://example.test/alpha");

  const blocking = await downloads.start({ ref: "curated:model:alpha" });
  await jobs.started(1);
  const big = await downloads.start({ ref: "curated:model:big" });
  const small = await downloads.start({ ref: "curated:mcp:small" });

  // One slot, one transfer: neither of the queued items has touched the network.
  expect(jobs.attempts).toHaveLength(1);
  expect(downloads.get(big.id).status).toBe("queued");
  expect(downloads.get(small.id).status).toBe("queued");
  expect(downloads.get(small.id).priority).toBeGreaterThan(downloads.get(big.id).priority);

  jobs.release("https://example.test/alpha");
  await settle(blocking.id);
  await settle(small.id);
  await settle(big.id);

  expect(jobs.attempts.map((attempt) => attempt.url)).toEqual([
    "https://example.test/alpha",
    "https://example.test/small",
    "https://example.test/big",
  ]);
});

test("a download with nowhere to put it is rejected before a byte moves", async () => {
  items = [item({ ref: "curated:model:huge", name: "huge", sizeBytes: 380 * GIB })];
  jobs.serve("https://example.test/huge", BODY);
  freeBytes = 12 * GIB;

  await expect(downloads.start({ ref: "curated:model:huge" })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
  expect(jobs.attempts).toHaveLength(0);
  expect(downloads.list()).toHaveLength(0);

  // The catalogue says the same thing before the user clicks, as the mockup does.
  const [row] = await downloads.catalogue({ refresh: false });
  expect(row?.state).toBe("available");
  expect(row?.downloadable).toBe(false);
  expect(row?.blockedReason).toMatch(/Недостаточно места/);

  freeBytes = 380 * GIB + FREE_SPACE_MARGIN_BYTES;
  const [freed] = await downloads.catalogue({ refresh: false });
  expect(freed?.downloadable).toBe(true);
});

test("the catalogue marks installed items and flags a newer version as an update", async () => {
  items = [
    item({ ref: "curated:model:alpha", name: "alpha", version: "q4_K_M", digest: "aa11" }),
    item({ ref: "curated:model:beta", name: "beta", version: "v2" }),
  ];
  present = [
    { kind: "model", name: "alpha", sizeBytes: 10, digest: "bb22" },
    { kind: "model", name: "beta", sizeBytes: 10, version: "v2" },
  ];

  const rows = await downloads.catalogue({ refresh: true });
  const byRef = new Map(rows.map((row) => [row.ref, row]));
  expect(byRef.get("curated:model:alpha")?.state).toBe("update");
  expect(byRef.get("curated:model:beta")?.state).toBe("installed");
  expect(byRef.get("curated:model:beta")?.downloadable).toBe(false);
});

test("the catalogue reports a queued item as in flight", async () => {
  jobs.serve("https://example.test/alpha", BODY, 6);
  jobs.hold("https://example.test/alpha");
  const started = await downloads.start({ ref: "curated:model:alpha" });
  await jobs.started(1);

  const [row] = await downloads.catalogue({ refresh: false });
  expect(row?.state).toBe("downloading");
  expect(row?.downloadId).toBe(started.id);

  jobs.release("https://example.test/alpha");
  await settle(started.id);
});

test("disk accounting totals each category and leaves the rest as other", async () => {
  mkdirSync(join(downloadsDir, "models"), { recursive: true });
  mkdirSync(join(downloadsDir, "mcp"), { recursive: true });
  writeFileSync(join(downloadsDir, "models", "a.bin"), "x".repeat(300));
  writeFileSync(join(downloadsDir, "mcp", "b.tgz"), "y".repeat(50));
  writeFileSync(join(vectorStoresDir, "index.lance"), "z".repeat(120));
  writeFileSync(join(workspace.path, "studio.sqlite"), "q".repeat(70));

  const usage = await downloads.disk(true);
  const bytes = new Map(usage.categories.map((entry) => [entry.category, entry.bytes]));
  expect(bytes.get("models")).toBe(300);
  expect(bytes.get("mcp")).toBe(50);
  expect(bytes.get("embeddings")).toBe(0);
  expect(bytes.get("vectors")).toBe(120);
  // Everything under userData that is not a tracked category.
  expect(bytes.get("other")).toBe(70);
  expect(usage.freeBytes).toBe(freeBytes);
  expect(usage.usedBytes).toBe(1000 * GIB - freeBytes);
});

test("a download interrupted by a restart is requeued and resumes from its partial", async () => {
  jobs.serve("https://example.test/alpha", BODY);
  const target = targetOf(items[0]!);
  mkdirSync(join(target, ".."), { recursive: true });
  writeFileSync(`${target}.part`, BODY.slice(0, 20));
  const at = clock.now();
  db.client.repositories.downloads.create({
    id: "0193c3c0-0000-7000-8000-000000000001",
    itemKind: "model",
    itemRef: "curated:model:alpha",
    displayName: "alpha",
    sizeBytes: BODY.length,
    bytesDone: 20,
    status: "running",
    priority: 5,
    targetPath: target,
    url: "https://example.test/alpha",
    createdAt: at,
    updatedAt: at,
  });

  expect(downloads.recover()).toBe(1);
  const finished = await settle("0193c3c0-0000-7000-8000-000000000001");

  expect(finished.status).toBe("succeeded");
  expect(jobs.attempts[0]?.resumedFrom).toBe(20);
  expect(readFileSync(target, "utf8")).toBe(BODY);
});

test("removing a finished download deletes the artefact, and an active one refuses", async () => {
  jobs.serve("https://example.test/alpha", BODY, 6);
  jobs.hold("https://example.test/alpha");
  const started = await downloads.start({ ref: "curated:model:alpha" });
  await jobs.started(1);

  await expect(downloads.remove(started.id)).rejects.toBeInstanceOf(AppError);

  jobs.release("https://example.test/alpha");
  await settle(started.id);
  expect(existsSync(started.targetPath)).toBe(true);

  await downloads.remove(started.id);
  expect(existsSync(started.targetPath)).toBe(false);
  expect(downloads.list()).toHaveLength(0);
});

test("the same item cannot be queued twice", async () => {
  jobs.serve("https://example.test/alpha", BODY, 6);
  jobs.hold("https://example.test/alpha");
  const started = await downloads.start({ ref: "curated:model:alpha" });
  await jobs.started(1);

  await expect(downloads.start({ ref: "curated:model:alpha" })).rejects.toMatchObject({
    code: AppErrorCode.CONFLICT,
  });
  await expect(downloads.start({ ref: "curated:model:nope" })).rejects.toMatchObject({
    code: AppErrorCode.NOT_FOUND,
  });

  jobs.release("https://example.test/alpha");
  await settle(started.id);
});

test("every download channel reaches the service", async () => {
  jobs.serve("https://example.test/alpha", BODY);
  const handlers = createDownloadHandlers(downloads);
  const started = await handlers["downloads.start"]({ ref: "curated:model:alpha" });
  await settle(started.id);

  expect(await handlers["downloads.list"]({})).toHaveLength(1);
  expect(await handlers["downloads.catalogue"]({ refresh: false })).toHaveLength(1);
  expect((await handlers["downloads.disk"]({ refresh: true })).root).toBe(downloadsDir);
  expect(await handlers["downloads.remove"]({ id: started.id })).toEqual({
    id: started.id,
    removed: true,
  });

  const missing = createDownloadHandlers(undefined);
  expect(() => missing["downloads.list"]({})).toThrow(AppError);
});
