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
  if (globalThis.location.href !== command.page) throw new Error("The gateway page moved");
  if (state.requests.has(command.id)) throw new Error("Duplicate gateway request");

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

  void (async () => {
    try {
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
      response.headers.forEach((value, name) => {
        if (name !== "set-cookie" && name !== "set-cookie2") headers.push([name, value]);
      });
      post({ id: command.id, kind: "head", status: response.status, headers });
      if (response.body !== null) {
        const reader = response.body.getReader();
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          for (let offset = 0; offset < value.length; offset += command.chunk) {
            await spend();
            const data = encode(value.subarray(offset, offset + command.chunk));
            post({ id: command.id, kind: "chunk", data });
          }
        }
      }
      post({ id: command.id, kind: "end" });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      post({ id: command.id, kind: "error", aborted: controller.signal.aborted, message });
    } finally {
      state.requests.delete(command.id);
    }
  })();
  return true;
}
