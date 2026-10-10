const allowed = new Set(["ipc_call", "events_subscribe", "plugin:__TAURI_CHANNEL__|fetch"]);

globalThis.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (payload !== null && typeof payload === "object" && allowed.has(payload.cmd)) return payload;
  throw new Error("Blocked by the isolation pattern");
};
