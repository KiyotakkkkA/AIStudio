// A framed Copilot server for host tests. Never reads credentials or performs network I/O.
import { Buffer } from "node:buffer";
import process from "node:process";
let buffer = Buffer.alloc(0);
const scenario = process.argv[2];
function send(message) {
  const body = JSON.stringify({ jsonrpc: "2.0", ...message });
  process.stdout.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
}
process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const end = buffer.indexOf("\r\n\r\n");
    if (end < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, end).toString())[1]);
    if (buffer.length < end + 4 + length) return;
    const request = JSON.parse(buffer.subarray(end + 4, end + 4 + length).toString());
    buffer = buffer.subarray(end + 4 + length);
    let result = {};
    if (request.method === "ping") result = { protocolVersion: scenario === "old" ? 2 : 3 };
    if (request.method === "auth.getStatus")
      result = { isAuthenticated: scenario !== "signed-out", login: "octocat", host: "github.com" };
    if (request.method === "models.list")
      result = {
        models: [
          {
            id: "model",
            name: "Model",
            capabilities: {
              supports: { reasoningEffort: true },
              limits: { max_context_window_tokens: 128000 },
            },
            supportedReasoningEfforts: ["low", "high"],
          },
        ],
      };
    if (request.method === "session.create") result = { sessionId: "session" };
    if (request.method === "session.options.update") {
      const isolated =
        request.params.enableHostGitOperations === false &&
        request.params.manageScheduleEnabled === false &&
        request.params.memory?.enabled === false &&
        request.params.skipCustomInstructions === true &&
        request.params.installedPlugins?.length === 0;
      if (!isolated) process.exit(3);
    }
    if (request.method === "session.send") {
      if (scenario === "permission") {
        send({ id: "permission", method: "permission.request", params: { kind: "shell" } });
      }
      for (const event of [
        { type: "assistant.message_delta", data: { deltaContent: "Ответ" } },
        { type: "session.idle", data: {} },
      ])
        send({ method: "session.event", params: { sessionId: "session", event } });
    }
    if (request.id === "permission") {
      if (request.result?.kind !== "denied-no-approval-rule-and-could-not-request-from-user")
        process.exit(2);
      continue;
    }
    if (request.method) send({ id: request.id, result });
  }
});
