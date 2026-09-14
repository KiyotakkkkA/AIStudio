// A stand-in for zvs-jobd that speaks the same newline-delimited JSON protocol, so the host
// driver can be tested without building Rust. Three job names drive the awkward cases:
// "ok" finishes after params.delayMs, "boom" exits mid-job, "hang" never finishes and ignores
// cancellation. Passing "deaf" as the first argument makes it stop answering ping.
import { createInterface } from "node:readline";
import process from "node:process";
import { clearTimeout, setTimeout } from "node:timers";

const deaf = process.argv[2] === "deaf";
const running = new Map();
const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);

createInterface({ input: process.stdin }).on("line", (line) => {
  if (!line.trim()) return;
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    send({ type: "job.error", id: "", code: "VALIDATION_FAILED", message: "Unreadable request" });
    return;
  }
  if (request.type === "ping") {
    if (!deaf) send({ type: "ping", id: request.id });
    return;
  }
  if (request.type === "job.cancel") {
    const timer = running.get(request.id);
    if (!timer) return;
    clearTimeout(timer);
    running.delete(request.id);
    send({ type: "job.error", id: request.id, code: "RUN_CANCELLED", message: "Отменено" });
    return;
  }
  if (request.type !== "job.start") return;
  const { id, job, params } = request;
  send({ type: "job.progress", id, done: 0, total: 2, message: job });
  if (job === "boom") {
    process.exit(1);
  }
  if (job === "hang") {
    return;
  }
  const timer = setTimeout(
    () => {
      running.delete(id);
      send({ type: "job.progress", id, done: 2, total: 2 });
      send({ type: "job.done", id, result: { job, params: params ?? null } });
    },
    Number(params?.delayMs ?? 0),
  );
  running.set(id, timer);
});
