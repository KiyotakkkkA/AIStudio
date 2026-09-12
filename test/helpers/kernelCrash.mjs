import process from "node:process";
import { readFileSync } from "node:fs";
import { createRequire, registerHooks } from "node:module";
import { setInterval } from "node:timers";
import { fileURLToPath, URL } from "node:url";
import ts from "typescript";

registerHooks({
  load(url, context, nextLoad) {
    if (!url.endsWith(".ts")) return nextLoad(url, context);
    return {
      format: "module",
      shortCircuit: true,
      source: ts.transpileModule(readFileSync(fileURLToPath(url), "utf8"), {
        compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext },
      }).outputText,
    };
  },
});

const studioRequire = createRequire(new URL("../../apps/studio/package.json", import.meta.url));
const { z } = studioRequire("zod");
const { StartRunInput } = await import("@zvs/shared");
const { openDatabase } = await import("../../apps/studio/src/host/data/client.ts");
const { NodeRegistry } = await import("../../apps/studio/src/host/kernel/NodeRegistry.ts");
const { RunService } = await import("../../apps/studio/src/host/services/RunService.ts");
const { createEventBus } = await import("../../apps/studio/src/host/platform/events.ts");
const data = openDatabase({ file: process.argv[2] });
const registry = new NodeRegistry();
registry.register({
  type: "crash.work",
  input: z.string(),
  output: z.string(),
  sideEffectFree: process.argv[3] === "safe",
  permission: { kind: "none" },
  async run(context, input) {
    if (input === "checkpoint") return "saved";
    process.send({ id: context.runId });
    return new Promise(() => {});
  },
});
const service = new RunService({ data, registry, events: createEventBus() });
service.start(
  StartRunInput.parse({
    kind: "job",
    graph: {
      nodes: [
        { id: "a", type: "crash.work", input: "checkpoint" },
        { id: "b", type: "crash.work", input: "wait", dependencies: ["a"] },
      ],
    },
  }),
);
setInterval(() => {}, 1000);
