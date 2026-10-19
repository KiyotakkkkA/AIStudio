import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { afterAll, afterEach, beforeAll, beforeEach, expect, test } from "vitest";
import gateway from "../src-tauri/src/browser/gateway/page.js";

type Command = Record<string, unknown>;

interface Message {
  readonly id: string;
  readonly kind: "head" | "chunk" | "end" | "error";
  readonly status?: number;
  readonly headers?: [string, string][];
  readonly final_url?: string | null;
  readonly redirected?: boolean;
  readonly data?: string;
  readonly aborted?: boolean;
  readonly message?: string;
}

const scope = globalThis as Record<string, unknown>;

const big = Buffer.from(Array.from({ length: 200_000 }, (_, index) => (index * 31) % 251));
const holds = new Set<ServerResponse>();
let server: Server;
let origin = "";
let page = "";
let messages: Message[] = [];
let waiters: (() => void)[] = [];

function body(request: IncomingMessage): Promise<string> {
  return new Promise((resolve) => {
    const parts: Buffer[] = [];
    request.on("data", (part: Buffer) => parts.push(part));
    request.on("end", () => resolve(Buffer.concat(parts).toString("utf8")));
  });
}

beforeAll(async () => {
  server = createServer((request, response) => {
    void (async () => {
      const path = request.url ?? "/";
      if (path === "/robots.txt") {
        response.writeHead(200, { "content-type": "text/plain" }).end("User-agent: *\n");
      } else if (path === "/echo") {
        const text = await body(request);
        response.writeHead(200, {
          "content-type": "application/json",
          "set-cookie": "fresh=1; HttpOnly",
          "x-trace": "abc",
        });
        response.end(
          JSON.stringify({
            method: request.method,
            authorization: request.headers.authorization ?? null,
            referer: request.headers.referer ?? null,
            body: text,
          }),
        );
      } else if (path === "/redirect") {
        response.writeHead(302, { location: "/login" }).end();
      } else if (path === "/login") {
        response.writeHead(200, { "content-type": "text/html" }).end("<html>Sign in</html>");
      } else if (path === "/big") {
        response.writeHead(200, { "content-type": "application/octet-stream" }).end(big);
      } else if (path === "/busy") {
        response.writeHead(503, { "content-type": "application/json" }).end('{"msg":"busy"}');
      } else if (path === "/sse") {
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.write("data: one\n\n");
        holds.add(response);
      } else {
        response.writeHead(404).end();
      }
    })();
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  page = `${origin}/robots.txt`;
});

afterAll(async () => {
  for (const response of holds) response.destroy();
  await new Promise<void>((resolve) => server.close(() => resolve()));
});

beforeEach(() => {
  messages = [];
  waiters = [];
  delete scope.__zvsGatewayState;
  scope.__zvsGatewaySend = (payload: string): void => {
    messages.push(JSON.parse(payload) as Message);
    for (const wake of waiters.splice(0)) wake();
  };
  scope.location = { href: page };
  scope.document = { readyState: "complete" };
});

afterEach(() => {
  delete scope.__zvsGatewaySend;
  delete scope.__zvsGatewayState;
  delete scope.location;
  delete scope.document;
});

async function until(predicate: () => boolean): Promise<void> {
  while (!predicate()) {
    await new Promise<void>((resolve) => {
      waiters.push(resolve);
      setTimeout(resolve, 50);
    });
  }
}

function start(id: string, path: string, extra: Command = {}): unknown {
  return gateway({
    op: "start",
    id,
    url: `${origin}${path}`,
    method: "GET",
    headers: [],
    body: null,
    referrer: null,
    page,
    chunk: 4096,
    window: 1000,
    ...extra,
  });
}

function settled(id: string): boolean {
  return messages.some(
    (message) => message.id === id && (message.kind === "end" || message.kind === "error"),
  );
}

function decoded(id: string): Buffer {
  return Buffer.concat(
    messages
      .filter((message) => message.id === id && message.kind === "chunk")
      .map((message) => Buffer.from(message.data ?? "", "base64")),
  );
}

test("the probe reports the page and the binding stays captured", () => {
  expect(gateway({ op: "probe" })).toEqual({ href: page, readyState: "complete" });
  delete scope.__zvsGatewaySend;
  expect(gateway({ op: "probe" })).toEqual({ href: page, readyState: "complete" });
  delete scope.__zvsGatewayState;
  expect(() => gateway({ op: "probe" })).toThrow("The gateway binding is missing");
});

test("a request runs with its method, headers, body and referrer, and hides cookies", async () => {
  expect(
    start("echo", "/echo", {
      method: "POST",
      headers: [
        ["authorization", "Bearer tok"],
        ["content-type", "application/json"],
      ],
      body: '{"a":1}',
      referrer: `${origin}/`,
    }),
  ).toBe(true);
  await until(() => settled("echo"));
  const head = messages.find((message) => message.kind === "head");
  expect(head?.status).toBe(200);
  expect(head?.final_url).toBe(`${origin}/echo`);
  expect(head?.redirected).toBe(false);
  expect(head?.headers).toContainEqual(["x-trace", "abc"]);
  expect(head?.headers?.some(([name]) => name === "set-cookie")).toBe(false);
  expect(JSON.parse(decoded("echo").toString("utf8"))).toEqual({
    method: "POST",
    authorization: "Bearer tok",
    referer: `${origin}/`,
    body: '{"a":1}',
  });
  expect(messages.at(-1)).toEqual({ id: "echo", kind: "end" });
});

test("a redirect preserves the final login URL alongside the HTML response", async () => {
  start("redirect", "/redirect");
  await until(() => settled("redirect"));
  expect(messages[0]).toMatchObject({
    kind: "head",
    status: 200,
    final_url: `${origin}/login`,
    redirected: true,
  });
  expect(decoded("redirect").toString("utf8")).toBe("<html>Sign in</html>");
});

test("Qwen permits SPA paths on its own origin but rejects other origins", async () => {
  scope.location = { href: "https://chat.qwen.ai/c/chat-id" };
  expect(start("qwen", "/echo", { page: "https://chat.qwen.ai/" })).toBe(true);
  await until(() => settled("qwen"));
  scope.location = { href: "https://other.example/" };
  expect(() => start("other", "/echo", { page: "https://chat.qwen.ai/" })).toThrow(
    "The gateway page moved",
  );
});

test("a bearer request uses the page's current token and cookie-only requests stay so", async () => {
  scope.localStorage = { getItem: (key: string) => (key === "token" ? "page-token" : null) };
  try {
    start("paged", "/echo", {
      headers: [["authorization", "Bearer stale"]],
      bearer_key: "token",
    });
    await until(() => settled("paged"));
    expect(JSON.parse(decoded("paged").toString("utf8")).authorization).toBe("Bearer page-token");
    start("cookies", "/echo", { headers: [], bearer_key: "token" });
    await until(() => settled("cookies"));
    expect(JSON.parse(decoded("cookies").toString("utf8")).authorization).toBeNull();
  } finally {
    delete scope.localStorage;
  }
});

test("a large body arrives in bounded chunks that wait for credit", async () => {
  start("big", "/big", { chunk: 16_384, window: 2 });
  await until(() => messages.filter((message) => message.kind === "chunk").length === 2);
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(messages.filter((message) => message.kind === "chunk")).toHaveLength(2);
  while (!settled("big")) {
    expect(gateway({ op: "ack", id: "big", credit: 2 })).toBe(true);
    const before = messages.length;
    await until(() => messages.length > before || settled("big"));
  }
  const chunks = messages.filter((message) => message.kind === "chunk");
  expect(
    chunks.every((message) => Buffer.from(message.data ?? "", "base64").length <= 16_384),
  ).toBe(true);
  expect(decoded("big").equals(big)).toBe(true);
  expect(gateway({ op: "ack", id: "big", credit: 2 })).toBe(false);
});

test("a failing status still streams its body", async () => {
  start("busy", "/busy");
  await until(() => settled("busy"));
  expect(messages[0]).toMatchObject({ kind: "head", status: 503 });
  expect(decoded("busy").toString("utf8")).toBe('{"msg":"busy"}');
});

test("an event stream is forwarded before it ends and aborting stops it", async () => {
  start("sse", "/sse");
  await until(() => messages.some((message) => message.kind === "chunk"));
  expect(decoded("sse").toString("utf8")).toBe("data: one\n\n");
  for (const response of holds) response.write("data: two\n\n");
  await until(() => decoded("sse").toString("utf8").includes("two"));
  expect(settled("sse")).toBe(false);
  expect(gateway({ op: "abort", id: "sse" })).toBe(true);
  await until(() => settled("sse"));
  expect(messages.at(-1)).toMatchObject({ id: "sse", kind: "error", aborted: true });
  expect(gateway({ op: "abort", id: "sse" })).toBe(false);
});

test("an abort while waiting for credit ends the request", async () => {
  start("stalled", "/big", { chunk: 1024, window: 1 });
  await until(() => messages.some((message) => message.kind === "chunk"));
  gateway({ op: "abort", id: "stalled" });
  await until(() => settled("stalled"));
  expect(messages.at(-1)).toMatchObject({ kind: "error", aborted: true });
});

test("a network failure is reported without aborting", async () => {
  start("down", "/", { url: "http://127.0.0.1:1/" });
  await until(() => settled("down"));
  expect(messages).toEqual([
    expect.objectContaining({ id: "down", kind: "error", aborted: false }),
  ]);
});

test("the page refuses requests once it moved, duplicates and unknown commands", async () => {
  start("first", "/sse");
  expect(() => start("first", "/sse")).toThrow("Duplicate gateway request");
  expect(() => gateway({ op: "teleport" })).toThrow("Unknown gateway command");
  scope.location = { href: `${origin}/elsewhere` };
  expect(() => start("moved", "/echo")).toThrow("The gateway page moved");
  gateway({ op: "abort", id: "first" });
  await until(() => settled("first"));
});

class FakeXhr {
  static last: FakeXhr | undefined;
  sent: { method: string; url: string; headers: [string, string][]; body: unknown } = {
    method: "",
    url: "",
    headers: [],
    body: null,
  };
  withCredentials = false;
  responseType = "";
  status = 0;
  response: ArrayBuffer | null = null;
  responseURL = "";
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  ontimeout: (() => void) | null = null;
  onabort: (() => void) | null = null;

  open(method: string, url: string): void {
    this.sent.method = method;
    this.sent.url = url;
  }

  setRequestHeader(name: string, value: string): void {
    this.sent.headers.push([name, value]);
  }

  send(body: unknown): void {
    this.sent.body = body;
    FakeXhr.last = this;
  }

  abort(): void {
    this.onabort?.();
  }

  getAllResponseHeaders(): string {
    return "Content-Type: application/json\r\nSet-Cookie: fresh=1\r\n";
  }

  answer(status: number, text: string): void {
    this.status = status;
    this.response = new TextEncoder().encode(text).buffer;
    this.responseURL = this.sent.url;
    this.onload?.();
  }
}

test("an xhr request goes through the page's XMLHttpRequest and hides cookies", async () => {
  scope.XMLHttpRequest = FakeXhr;
  try {
    start("xhr", "/api/v2/chats/new", {
      transport: "xhr",
      method: "POST",
      headers: [["content-type", "application/json"]],
      body: '{"chat_mode":"normal"}',
    });
    const xhr = FakeXhr.last;
    expect(xhr?.sent).toEqual({
      method: "POST",
      url: `${origin}/api/v2/chats/new`,
      headers: [["content-type", "application/json"]],
      body: '{"chat_mode":"normal"}',
    });
    expect(xhr?.withCredentials).toBe(true);
    xhr?.answer(200, '{"success":true}');
    await until(() => settled("xhr"));
    expect(messages[0]).toEqual({
      id: "xhr",
      kind: "head",
      status: 200,
      headers: [["content-type", "application/json"]],
      final_url: `${origin}/api/v2/chats/new`,
      redirected: false,
    });
    expect(decoded("xhr").toString("utf8")).toBe('{"success":true}');
    expect(messages.at(-1)).toEqual({ id: "xhr", kind: "end" });

    start("aborted", "/api/v2/chats/new", { transport: "xhr" });
    gateway({ op: "abort", id: "aborted" });
    await until(() => settled("aborted"));
    expect(messages.at(-1)).toMatchObject({ id: "aborted", kind: "error", aborted: true });
  } finally {
    delete scope.XMLHttpRequest;
  }
});

test("a verification dialog added while waiting is reported once", async () => {
  let notify: ((records: { addedNodes: unknown[] }[]) => void) | undefined;
  let disconnected = false;
  const element = (attributes: Record<string, string>, children: unknown[] = []) => ({
    nodeType: 1,
    getAttribute: (name: string) => attributes[name] ?? null,
    querySelectorAll: () => children,
  });
  scope.document = { readyState: "complete", body: {} };
  scope.MutationObserver = class {
    constructor(callback: (records: { addedNodes: unknown[] }[]) => void) {
      notify = callback;
    }
    observe(): void {}
    disconnect(): void {
      disconnected = true;
    }
  };
  scope.XMLHttpRequest = FakeXhr;
  try {
    start("watched", "/api/v2/chats/new", { transport: "xhr", challenge: "baxia|punish" });
    notify?.([{ addedNodes: [element({ class: "toast" })] }]);
    expect(messages).toEqual([]);
    const dialog = element({ id: "wrapper" }, [element({ id: "baxia-dialog-content" })]);
    notify?.([{ addedNodes: [dialog] }]);
    notify?.([{ addedNodes: [element({ src: "https://x/punish?x5" })] }]);
    expect(messages).toEqual([{ id: "watched", kind: "challenge" }]);
    FakeXhr.last?.answer(200, "{}");
    await until(() => settled("watched"));
    expect(disconnected).toBe(true);
  } finally {
    delete scope.MutationObserver;
    delete scope.XMLHttpRequest;
  }
});
