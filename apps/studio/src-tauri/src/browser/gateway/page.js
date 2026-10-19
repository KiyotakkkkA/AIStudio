export default function gateway(command) {
  const binding = "__zvsGatewaySend";
  const state = (globalThis.__zvsGatewayState ??= {
    requests: new Map(),
    send: globalThis[binding],
  });
  if (typeof state.send !== "function") throw new Error("The gateway binding is missing");
  const post = (message) => state.send(JSON.stringify(message));

  if (command.op === "probe") {
    return { href: globalThis.location.href, readyState: globalThis.document.readyState };
  }
  if (command.op === "ack" || command.op === "abort") {
    const entry = state.requests.get(command.id);
    if (entry === undefined) return false;
    if (command.op === "ack") entry.credit += command.credit;
    else entry.controller.abort();
    entry.wake?.();
    return true;
  }
  if (command.op !== "start") throw new Error("Unknown gateway command");
  const sameQwenOrigin =
    command.page === "https://chat.qwen.ai/" &&
    new globalThis.URL(globalThis.location.href).origin === "https://chat.qwen.ai";
  if (globalThis.location.href !== command.page && !sameQwenOrigin) {
    throw new Error("The gateway page moved");
  }
  if (state.requests.has(command.id)) throw new Error("Duplicate gateway request");

  // A request that carries a bearer uses the one the vendor's own app keeps current in the page,
  // as the app's requests do; the token never leaves the page. Cookie-only requests stay so.
  if (typeof command.bearer_key === "string" && Array.isArray(command.headers)) {
    const authorised = command.headers.some(([name]) => name.toLowerCase() === "authorization");
    let current = null;
    try {
      current = globalThis.localStorage?.getItem(command.bearer_key) ?? null;
    } catch {
      current = null;
    }
    if (authorised && typeof current === "string" && current.length > 0) {
      command = {
        ...command,
        headers: [
          ...command.headers.filter(([name]) => name.toLowerCase() !== "authorization"),
          ["authorization", `Bearer ${current}`],
        ],
      };
    }
  }

  const controller = new globalThis.AbortController();
  const entry = { controller, credit: command.window, wake: null };
  state.requests.set(command.id, entry);

  const encode = (bytes) => {
    let binary = "";
    for (let offset = 0; offset < bytes.length; offset += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
    }
    return globalThis.btoa(binary);
  };

  const spend = async () => {
    while (entry.credit <= 0) {
      controller.signal.throwIfAborted();
      await new Promise((resolve) => {
        entry.wake = resolve;
      });
      entry.wake = null;
    }
    controller.signal.throwIfAborted();
    entry.credit -= 1;
  };

  // Reports, once, an element the vendor adds to ask a person to verify. Only the fact is
  // posted; the host decides whether to show the page.
  const watchChallenge = () => {
    const document = globalThis.document;
    const Observer = globalThis.MutationObserver;
    if (typeof command.challenge !== "string" || !document?.body || !Observer) return () => {};
    const pattern = new RegExp(command.challenge, "i");
    const matches = (element) =>
      typeof element.getAttribute === "function" &&
      ["id", "class", "src"].some((name) => pattern.test(element.getAttribute(name) ?? ""));
    const contains = (root) =>
      matches(root) ||
      [...(root.querySelectorAll?.("[id],[class],iframe[src]") ?? [])].some(matches);
    let reported = false;
    const report = () => {
      if (reported) return;
      reported = true;
      post({ id: command.id, kind: "challenge" });
    };
    const observer = new Observer((records) => {
      for (const record of records) {
        if ([...record.addedNodes].some((node) => node.nodeType === 1 && contains(node))) report();
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  };

  const viaFetch = async () => {
    const init = {
      method: command.method,
      headers: command.headers,
      credentials: "include",
      cache: "no-store",
      redirect: "follow",
      signal: controller.signal,
    };
    if (typeof command.body === "string") init.body = command.body;
    if (typeof command.referrer === "string") init.referrer = command.referrer;
    const response = await globalThis.fetch(command.url, init);
    const headers = [];
    response.headers.forEach((value, name) => headers.push([name, value]));
    const reader = response.body?.getReader();
    return {
      status: response.status,
      headers,
      final_url: response.url || null,
      redirected: response.redirected,
      read: reader ? () => reader.read() : async () => ({ done: true }),
    };
  };

  // XHR follows redirects and sends credentials for same-origin URLs; the referrer is the
  // page's own location.
  const viaXhr = () =>
    new Promise((resolve, reject) => {
      const xhr = new globalThis.XMLHttpRequest();
      xhr.open(command.method, command.url, true);
      xhr.withCredentials = true;
      xhr.responseType = "arraybuffer";
      for (const [name, value] of command.headers) xhr.setRequestHeader(name, value);
      const onAbort = () => xhr.abort();
      controller.signal.addEventListener("abort", onAbort, { once: true });
      xhr.onload = () => {
        controller.signal.removeEventListener("abort", onAbort);
        const headers = xhr
          .getAllResponseHeaders()
          .trim()
          .split(/[\r\n]+/)
          .filter(Boolean)
          .map((line) => {
            const colon = line.indexOf(":");
            return [line.slice(0, colon).trim().toLowerCase(), line.slice(colon + 1).trim()];
          });
        let body = new Uint8Array(xhr.response ?? new ArrayBuffer(0));
        resolve({
          status: xhr.status,
          headers,
          final_url: xhr.responseURL || null,
          redirected: Boolean(xhr.responseURL) && xhr.responseURL !== command.url,
          read: async () => {
            if (body === null || body.length === 0) return { done: true };
            const value = body;
            body = null;
            return { done: false, value };
          },
        });
      };
      xhr.onerror = () => reject(new Error("The request failed"));
      xhr.ontimeout = xhr.onerror;
      xhr.onabort = () =>
        reject(new globalThis.DOMException("The request was aborted", "AbortError"));
      xhr.send(typeof command.body === "string" ? command.body : null);
    });

  void (async () => {
    const unwatch = watchChallenge();
    try {
      const response = await (command.transport === "xhr" ? viaXhr() : viaFetch());
      unwatch();
      post({
        id: command.id,
        kind: "head",
        status: response.status,
        headers: response.headers.filter(
          ([name]) => name !== "set-cookie" && name !== "set-cookie2",
        ),
        final_url: response.final_url,
        redirected: response.redirected,
      });
      for (;;) {
        const { done, value } = await response.read();
        if (done) break;
        for (let offset = 0; offset < value.length; offset += command.chunk) {
          await spend();
          const data = encode(value.subarray(offset, offset + command.chunk));
          post({ id: command.id, kind: "chunk", data });
        }
      }
      post({ id: command.id, kind: "end" });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      post({ id: command.id, kind: "error", aborted: controller.signal.aborted, message });
    } finally {
      unwatch();
      state.requests.delete(command.id);
    }
  })();
  return true;
}
