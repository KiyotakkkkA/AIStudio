import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";
import { AppErrorCode, LOCAL_BASE_URL } from "@zvs/shared";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import {
  FileSystemModelStore,
  LocalEmbeddingAdapter,
  LOCAL_CAPABILITIES,
} from "../src/host/drivers/ai/adapters/local.ts";
import { adapterEntry } from "../src/host/drivers/ai/adapters/index.ts";
import { ProviderRegistry } from "../src/host/drivers/ai/ProviderRegistry.ts";
import type { ProviderEntity } from "../src/host/data/schema/index.ts";

/** A saved local provider keeps whatever vendor the form defaulted to; the family decides. */
function localRow(): ProviderEntity {
  return {
    id: "0199dd11-1111-7111-8111-000000000001",
    kind: "ollama",
    adapter: "local",
    authMode: "api",
    name: "Мои модели",
    baseUrl: LOCAL_BASE_URL,
    secretId: null,
    accountId: null,
    capabilities: ["embedding"],
    settings: {},
    enabled: true,
    status: "unknown",
    statusDetail: null,
    lastProbeAt: null,
    lastLatencyMs: null,
    defaultModelId: null,
    createdAt: 1,
    updatedAt: 1,
  } as ProviderEntity;
}

let workspace: TemporaryDirectory;
let store: FileSystemModelStore;

function put(folder: string, name: string, bytes: number): void {
  const directory = join(workspace.path, folder);
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, name), Buffer.alloc(bytes));
}

beforeEach(() => {
  workspace = temporaryDirectory("local-models-");
  store = new FileSystemModelStore({ root: workspace.path });
});

afterEach(() => {
  workspace.dispose();
});

test("the local store lists the weight files on disk and ignores everything else", async () => {
  put("embeddings", "bge-m3-FP16.gguf", 64);
  put("models", "bge-reranker-v2-m3-FP16.gguf", 32);
  put("models", "notes.txt", 8);
  put("models", "qwen.gguf.part", 8);
  put("mcp", "server-filesystem.tgz", 8);

  const models = await store.list(AbortSignal.timeout(5_000));

  expect(models.map((model) => model.externalId)).toEqual([
    "bge-m3-FP16.gguf",
    "bge-reranker-v2-m3-FP16.gguf",
  ]);
  expect(models[0]?.displayName).toBe("bge-m3-FP16");
  expect(models[0]?.sizeBytes).toBe(64);
});

test("a missing downloads folder is an empty list, not a failure", async () => {
  expect(await store.list(AbortSignal.timeout(5_000))).toEqual([]);
});

test("the same file under two folders is listed once", async () => {
  put("models", "bge-m3-FP16.gguf", 16);
  put("embeddings", "bge-m3-FP16.gguf", 16);

  expect(await store.list(AbortSignal.timeout(5_000))).toHaveLength(1);
});

test("the local adapter discovers models and refuses to embed until an engine exists", async () => {
  put("embeddings", "bge-m3-FP16.gguf", 16);
  const adapter = new LocalEmbeddingAdapter(store);

  expect(adapter.capabilities()).toBe(LOCAL_CAPABILITIES);
  expect(adapter.dimensions()).toBeNull();
  expect(await adapter.listModels(AbortSignal.timeout(5_000))).toHaveLength(1);
  await expect(
    adapter.embed(["привет"], "bge-m3-FP16.gguf", AbortSignal.timeout(5_000)),
  ).rejects.toMatchObject({ code: AppErrorCode.CONFLICT });
});

test("with an engine attached, the adapter hands the model name straight to it", async () => {
  put("embeddings", "bge-m3-FP16.gguf", 16);
  const calls: { model: string; texts: readonly string[] }[] = [];
  const adapter = new LocalEmbeddingAdapter(store, {
    embed: (model, texts) => {
      calls.push({ model, texts });
      return Promise.resolve([Float32Array.from([1, 0])]);
    },
  });

  const vectors = await adapter.embed(["привет"], "bge-m3-FP16.gguf", AbortSignal.timeout(5_000));

  expect(vectors).toEqual([Float32Array.from([1, 0])]);
  expect(calls).toEqual([{ model: "bge-m3-FP16.gguf", texts: ["привет"] }]);
});

test("the registry entry builds an embedding-only driver and needs no transport", () => {
  const entry = adapterEntry("local");

  expect(entry.implemented).toBe(true);
  expect(entry.capabilities.embedding).toBe(true);
  expect(LOCAL_BASE_URL).toBe("local://models");

  const driver = entry.build({ localModels: store });
  expect(driver.text).toBeNull();
  expect(driver.image).toBeNull();
  expect(driver.embedding).not.toBeNull();

  expect(() => entry.build({})).toThrow();
});

test("the local family wins over the vendor, so nothing reaches the network", async () => {
  put("embeddings", "bge-m3-FP16.gguf", 16);
  const registry = new ProviderRegistry({
    providers: { findById: () => undefined },
    secrets: { resolve: () => Promise.reject(new Error("no secret should be resolved")) },
    localModels: store,
    fetch: () => Promise.reject(new Error("no request should leave the machine")),
  });

  const driver = await registry.ephemeralDriver(localRow());

  expect(driver.text).toBeNull();
  expect(await driver.embedding?.listModels(AbortSignal.timeout(5_000))).toHaveLength(1);
});
