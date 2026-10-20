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
  const readStorage = (key) => {
    try {
      return globalThis.localStorage?.getItem(key) ?? null;
    } catch {
      return null;
    }
  };
  const writeStorage = (key, value) => {
    try {
      globalThis.localStorage?.setItem(key, value);
      return true;
    } catch {
      return false;
    }
  };
  const claimsExpiry = (token) => {
    try {
      const part = token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
      const claims = JSON.parse(globalThis.atob(part));
      return typeof claims.exp === "number" ? claims.exp * 1000 : null;
    } catch {
      return null;
    }
  };
  const tokenExpiry = (session) => {
    const recorded = Number(readStorage(session.expiry_key));
    if (Number.isFinite(recorded) && recorded > 0) return recorded;
    const token = readStorage(session.key);
    return typeof token === "string" ? claimsExpiry(token) : null;
  };
  const announce = (name, stateId, sourceId) => {
    if (typeof globalThis.BroadcastChannel !== "function") return false;
    try {
      const channel = new globalThis.BroadcastChannel(name);
      channel.postMessage({ type: "TOKEN_UPDATED", stateId, sourceId, timestamp: Date.now() });
      channel.close();
      return true;
    } catch {
      return false;
    }
  };
  const object = (value) => (typeof value === "object" && value !== null ? value : null);
  const refreshSession = async (session) => {
    const before = readStorage(session.key);
    let response;
    try {
      response = await globalThis.fetch(session.url, {
        method: "GET",
        headers: [
          ...session.headers,
          ["timezone", new Date().toString().replace(/\s*\(.*\)$/, "")],
        ],
        credentials: "include",
        cache: "no-store",
        redirect: "follow",
      });
    } catch {
      return {
        status: 0,
        success: null,
        code: null,
        details: null,
        fields: [],
        stored: false,
        changed: false,
      };
    }
    let payload = null;
    try {
      payload = await response.json();
    } catch {
      payload = null;
    }
    const root = object(payload);
    const data = object(root?.data);
    const fields = [
      ...Object.keys(root ?? {}),
      ...Object.keys(data ?? {}).map((name) => `data.${name}`),
    ].slice(0, 24);
    const token = [data?.access_token, data?.token, root?.access_token, root?.token].find(
      (value) => typeof value === "string" && value.length > 0,
    );
    let written = false;
    if (response.ok && typeof token === "string") {
      const expiresAt =
        typeof data?.expires_at === "number" ? data.expires_at * 1000 : claimsExpiry(token);
      const stateId = globalThis.crypto.randomUUID();
      const sourceId = (state.sourceId ??= globalThis.crypto.randomUUID());
      written = writeStorage(session.key, token);
      if (written && expiresAt !== null) {
        writeStorage(session.expiry_key, String(expiresAt));
        writeStorage(
          session.state_key,
          JSON.stringify({
            version: 1,
            stateId,
            token,
            expiresAt,
            updatedAt: Date.now(),
            sourceId,
          }),
        );
      }
      if (written) announce(session.channel, stateId, sourceId);
    }
    const after = readStorage(session.key);
    return {
      status: response.status,
      success: typeof root?.success === "boolean" ? root.success : null,
      code: typeof data?.code === "string" ? data.code : null,
      details: typeof data?.details === "string" ? data.details.slice(0, 200) : null,
      fields,
      stored: written,
      changed: after !== null && after !== before,
    };
  };
  const renewSession = (session) =>
    (state.renewing ??= refreshSession(session).finally(() => {
      state.renewing = null;
    }));

  if (command.op === "renew") {
    return renewSession(command.session);
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
  const prepareHeaders = async () => {
    if (typeof command.bearer_key !== "string" || !Array.isArray(command.headers)) {
      return command.headers;
    }
    const authorised = command.headers.some(([name]) => name.toLowerCase() === "authorization");
    if (!authorised) return command.headers;
    const session = object(command.session);
    if (session !== null) {
      const expiry = tokenExpiry(session);
      if (expiry !== null && expiry - Date.now() < session.leeway_ms) await renewSession(session);
    }
    const current = readStorage(command.bearer_key);
    if (typeof current !== "string" || current.length === 0) return command.headers;
    return [
      ...command.headers.filter(([name]) => name.toLowerCase() !== "authorization"),
      ["authorization", `Bearer ${current}`],
    ];
  };

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

  const viaFetch = async (headers) => {
    const init = {
      method: command.method,
      headers,
      credentials: "include",
      cache: "no-store",
      redirect: "follow",
      signal: controller.signal,
    };
    if (typeof command.body === "string") init.body = command.body;
    if (typeof command.referrer === "string") init.referrer = command.referrer;
    const response = await globalThis.fetch(command.url, init);
    const received = [];
    response.headers.forEach((value, name) => received.push([name, value]));
    const reader = response.body?.getReader();
    return {
      status: response.status,
      headers: received,
      final_url: response.url || null,
      redirected: response.redirected,
      read: reader ? () => reader.read() : async () => ({ done: true }),
    };
  };

  // XHR follows redirects and sends credentials for same-origin URLs; the referrer is the
  // page's own location.
  const viaXhr = (headers) =>
    new Promise((resolve, reject) => {
      const xhr = new globalThis.XMLHttpRequest();
      xhr.open(command.method, command.url, true);
      xhr.withCredentials = true;
      xhr.responseType = "arraybuffer";
      for (const [name, value] of headers) xhr.setRequestHeader(name, value);
      const onAbort = () => xhr.abort();
      controller.signal.addEventListener("abort", onAbort, { once: true });
      xhr.onload = () => {
        controller.signal.removeEventListener("abort", onAbort);
        const received = xhr
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
          headers: received,
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
      const headers = await prepareHeaders();
      const response = await (command.transport === "xhr" ? viaXhr(headers) : viaFetch(headers));
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
