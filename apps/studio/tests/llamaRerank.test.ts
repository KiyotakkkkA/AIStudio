import { EventEmitter } from "node:events";
import { expect, test, vi } from "vitest";
import { LlamaServer } from "../src/host/runtimes/LlamaServer.ts";

const LLAMA_B10978_RERANK_RESPONSE = {
  model: "x",
  object: "list",
  usage: { prompt_tokens: 50, total_tokens: 50 },
  results: [
    { index: 1, relevance_score: 0.9417868852615356 },
    { index: 2, relevance_score: -0.22709690034389496 },
    { index: 0, relevance_score: -7.699778079986572 },
  ],
};

class FakeChild extends EventEmitter {
  exitCode: number | null = null;
  readonly stdout = Object.assign(new EventEmitter(), { setEncoding: () => undefined });
  readonly stderr = Object.assign(new EventEmitter(), { setEncoding: () => undefined });
  kill() {
    this.exitCode = 0;
    this.emit("exit", 0);
    return true;
  }
}

function response(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function rerankServer(
  answer: (url: string, init?: RequestInit) => Promise<Response>,
  health: () => Promise<Response> = () => Promise.resolve(response({ status: "ok" })),
) {
  const argv: string[][] = [];
  const children: FakeChild[] = [];
  const fetch = vi.fn((url: string, init?: RequestInit) =>
    url.endsWith("/health") ? health() : answer(url, init),
  );
  const server = new LlamaServer({
    executablePath: "llama-server.exe",
    modelPath: "C:/models/bge-reranker-v2-m3-FP16.gguf",
    role: "rerank",
    gpuLayers: 999,
    idleMs: 0,
    freePort: () => Promise.resolve(18733),
    fetch,
    spawn: ((_: string, args: string[]) => {
      argv.push(args);
      const child = new FakeChild();
      children.push(child);
      return child;
    }) as never,
  });
  return { server, argv, children, fetch };
}

test("the rerank role starts llama-server in reranking mode with a full micro-batch", async () => {
  const { server, argv } = rerankServer(() =>
    Promise.resolve(response(LLAMA_B10978_RERANK_RESPONSE)),
  );
  await server.rerank("what is a panda?", ["hi", "panda", "pandas"], AbortSignal.timeout(1_000));
  const args = argv[0]!;
  expect(args).toContain("--reranking");
  expect(args).not.toContain("--embeddings");
  expect(args).not.toContain("--mmproj");
  expect(args[args.indexOf("--ubatch-size") + 1]).toBe(args[args.indexOf("--batch-size") + 1]);
  server.stop();
});

test("scores come back in passage order, as the raw logits the build answers with", async () => {
  const { server, fetch } = rerankServer(() =>
    Promise.resolve(response(LLAMA_B10978_RERANK_RESPONSE)),
  );
  const scores = await server.rerank(
    "what is a panda?",
    ["hi", "The giant panda is a bear species endemic to China.", "pandas is a python library"],
    AbortSignal.timeout(1_000),
  );
  expect(scores).toEqual([-7.699778079986572, 0.9417868852615356, -0.22709690034389496]);
  const call = fetch.mock.calls.find(([url]) => url.endsWith("/v1/rerank"))!;
  expect(call[0]).toBe("http://127.0.0.1:18733/v1/rerank");
  const body = JSON.parse(String(call[1]?.body)) as Record<string, unknown>;
  expect(body.query).toBe("what is a panda?");
  expect(body.documents).toHaveLength(3);
  expect(body.top_n).toBe(3);
  server.stop();
});

test("an answer that misses a passage is rejected rather than half-applied", async () => {
  const { server } = rerankServer(() =>
    Promise.resolve(response({ results: [{ index: 0, relevance_score: 1 }] })),
  );
  await expect(server.rerank("q", ["a", "b"], AbortSignal.timeout(1_000))).rejects.toThrow(
    "Движок оценил не все фрагменты",
  );
  server.stop();
});

test("an out-of-range index or a non-numeric score is rejected", async () => {
  const { server, fetch } = rerankServer(() =>
    Promise.resolve(response({ results: [{ index: 3, relevance_score: 1 }] })),
  );
  await expect(server.rerank("q", ["a"], AbortSignal.timeout(1_000))).rejects.toThrow();
  fetch.mockImplementation((url: string) =>
    Promise.resolve(
      url.endsWith("/health")
        ? response({ status: "ok" })
        : response({ results: [{ index: 0, relevance_score: "high" }] }),
    ),
  );
  await expect(server.rerank("q", ["a"], AbortSignal.timeout(1_000))).rejects.toThrow();
  server.stop();
});

test("giving up on a cold start does not kill the model that is still loading", async () => {
  let ready = false;
  const { server, children } = rerankServer(
    () => Promise.resolve(response(LLAMA_B10978_RERANK_RESPONSE)),
    () => Promise.resolve(ready ? response({ status: "ok" }) : response({}, 503)),
  );
  const controller = new AbortController();
  const first = server.rerank("q", ["a", "b", "c"], controller.signal);
  await vi.waitFor(() => expect(children).toHaveLength(1));
  controller.abort(new Error("timed out"));
  await expect(first).rejects.toThrow("timed out");
  expect(server.running).toBe(true);
  ready = true;
  await expect(
    server.rerank("q", ["a", "b", "c"], AbortSignal.timeout(2_000)),
  ).resolves.toHaveLength(3);
  expect(children).toHaveLength(1);
  server.stop();
});

test("an empty passage list costs no process and no request", async () => {
  const { server, argv, fetch } = rerankServer(() => Promise.reject(new Error("unused")));
  await expect(server.rerank("q", [], AbortSignal.timeout(1_000))).resolves.toEqual([]);
  expect(argv).toHaveLength(0);
  expect(fetch).not.toHaveBeenCalled();
});

test("a server that died mid-request is started again on the next call", async () => {
  let calls = 0;
  const { server, children } = rerankServer(() => {
    calls += 1;
    return calls === 1
      ? Promise.reject(new TypeError("fetch failed"))
      : Promise.resolve(response(LLAMA_B10978_RERANK_RESPONSE));
  });
  await expect(server.rerank("q", ["a", "b", "c"], AbortSignal.timeout(1_000))).rejects.toThrow(
    "fetch failed",
  );
  await expect(
    server.rerank("q", ["a", "b", "c"], AbortSignal.timeout(1_000)),
  ).resolves.toHaveLength(3);
  expect(children).toHaveLength(2);
  server.stop();
});

test("a process that exits on its own is not mistaken for a running server", async () => {
  const { server, children } = rerankServer(() =>
    Promise.resolve(response(LLAMA_B10978_RERANK_RESPONSE)),
  );
  await server.rerank("q", ["a", "b", "c"], AbortSignal.timeout(1_000));
  children[0]!.kill();
  expect(server.running).toBe(false);
  await server.rerank("q", ["a", "b", "c"], AbortSignal.timeout(1_000));
  expect(children).toHaveLength(2);
  server.stop();
});
