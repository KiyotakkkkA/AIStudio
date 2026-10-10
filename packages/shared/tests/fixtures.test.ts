import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { z } from "zod";
import {
  AppErrorCode,
  contract,
  DeviceGpuDto,
  DeviceProfileDto,
  DeviceTier,
  HostEvent,
  Json,
  Page,
  PageRequest,
  ProviderId,
  ResourceSampleDto,
  Result,
  RunOutcomeDto,
  Sort,
  SortDirection,
  StreamId,
} from "../src/index.js";

const schemas: Record<string, z.ZodType> = {
  "primitives/ProviderId": ProviderId,
  "primitives/StreamId": StreamId,
  "primitives/PageRequest": PageRequest,
  "primitives/Page": Page(Json),
  "primitives/Sort": Sort(z.string()),
  "primitives/SortDirection": SortDirection,
  "errors/AppErrorCode": z.enum(AppErrorCode),
  "errors/ResultEnvelope": Result(Json),
  "events/RunOutcomeDto": RunOutcomeDto,
  "events/HostEvent": HostEvent,
  "system/DeviceTier": DeviceTier,
  "system/DeviceGpuDto": DeviceGpuDto,
  "system/DeviceProfileDto": DeviceProfileDto,
  "system/ResourceSampleDto": ResourceSampleDto,
  "system/SystemPingInput": contract["system.ping"].input,
  "system/SystemPingOutput": contract["system.ping"].output,
  "system/SystemDemoStreamInput": contract["system.demoStream"].input,
  "system/SystemDemoStreamOutput": contract["system.demoStream"].output,
  "system/SystemDemoJobInput": contract["system.demoJob"].input,
  "system/SystemDeviceInput": contract["system.device"].input,
  "system/SystemNativePingInput": contract["system.nativePing"].input,
  "system/SystemNativePingOutput": contract["system.nativePing"].output,
};

const root = fileURLToPath(new URL("../fixtures/", import.meta.url));

interface Fixture {
  name: string;
  kind: "valid" | "invalid";
  file: string;
}

const FixtureCases = z.array(z.object({ name: z.string(), value: z.unknown() })).min(1);

function collect(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? collect(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

const fixtures: Fixture[] = collect(root)
  .sort()
  .map((file) => {
    const path = relative(root, file).split(sep).join("/");
    const match = /^(.+)\.(valid|invalid)\.json$/.exec(path);
    if (!match?.[1] || !match[2]) {
      throw new Error(`${path} is neither <Name>.valid.json nor <Name>.invalid.json`);
    }
    return { name: match[1], kind: match[2] as Fixture["kind"], file };
  });

function cases(file: string): { name: string; value: unknown }[] {
  return FixtureCases.parse(JSON.parse(readFileSync(file, "utf8")));
}

describe("golden fixtures", () => {
  it("map every fixture to a Zod schema and every schema to a valid fixture", () => {
    const problems: string[] = [];
    for (const fixture of fixtures) {
      if (!(fixture.name in schemas)) {
        problems.push(`${fixture.name} has no entry in the Zod fixture table`);
      }
    }
    const valid = new Set(fixtures.filter((f) => f.kind === "valid").map((f) => f.name));
    for (const name of Object.keys(schemas)) {
      if (!valid.has(name)) problems.push(`${name} is in the table but has no ${name}.valid.json`);
    }
    expect(problems).toEqual([]);
  });

  for (const fixture of fixtures) {
    const schema = schemas[fixture.name];
    if (!schema) continue;
    describe(`${fixture.name}.${fixture.kind}`, () => {
      for (const { name, value } of cases(fixture.file)) {
        if (fixture.kind === "valid") {
          it(`accepts ${name}`, () => {
            const parsed = schema.safeParse(value);
            expect(parsed.error).toBeUndefined();
          });
        } else {
          it(`rejects ${name}`, () => {
            expect(schema.safeParse(value).success).toBe(false);
          });
        }
      }
    });
  }

  it("lists every AppErrorCode in order", () => {
    const listed = cases(join(root, "errors/AppErrorCode.valid.json")).map((c) => c.value);
    expect(listed).toEqual(Object.values(AppErrorCode));
  });
});
