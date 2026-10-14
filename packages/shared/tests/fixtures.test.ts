import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { z } from "zod";
import {
  AccountDto,
  AdapterCapabilitiesDto,
  AdapterDescriptorDto,
  AppErrorCode,
  BrowserBounds,
  BrowserCommand,
  contract,
  DeviceGpuDto,
  DeviceProfileDto,
  DeviceTier,
  DiscoveredModelDto,
  HostEvent,
  Json,
  ModelDto,
  Page,
  PageRequest,
  ProbeOutcomeDto,
  ProviderConnectionInput,
  ProviderDto,
  ProviderId,
  ProviderSettingsDto,
  ProviderSummaryDto,
  ResourceSampleDto,
  Result,
  RunOutcomeDto,
  SECRET_TYPE_REGISTRY,
  SecretDto,
  SecretSummaryDto,
  SecretTypeSchema,
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
  "settings/SettingsGetInput": contract["settings.get"].input,
  "settings/SettingsGetOutput": contract["settings.get"].output,
  "settings/SettingsSetInput": contract["settings.set"].input,
  "settings/SettingsSetOutput": contract["settings.set"].output,
  "secrets/SecretTypeSchema": SecretTypeSchema,
  "secrets/SecretsTypesOutput": contract["secrets.types"].output,
  "secrets/SecretFilter": contract["secrets.list"].input,
  "secrets/SecretRef": contract["secrets.get"].input,
  "secrets/CreateSecretInput": contract["secrets.create"].input,
  "secrets/UpdateSecretInput": contract["secrets.update"].input,
  "secrets/SecretSummaryDto": SecretSummaryDto,
  "secrets/SecretDto": SecretDto,
  "secrets/SecretsRemoveOutput": contract["secrets.remove"].output,
  "ai/AdapterCapabilitiesDto": AdapterCapabilitiesDto,
  "ai/AdapterDescriptorDto": AdapterDescriptorDto,
  "providers/ProviderSettingsDto": ProviderSettingsDto,
  "providers/DiscoveredModelDto": DiscoveredModelDto,
  "providers/ModelDto": ModelDto,
  "providers/ProviderSummaryDto": ProviderSummaryDto,
  "providers/ProviderDto": ProviderDto,
  "providers/ProviderConnectionInput": ProviderConnectionInput,
  "providers/CreateProviderInput": contract["providers.create"].input,
  "providers/UpdateProviderInput": contract["providers.update"].input,
  "providers/ProviderListFilter": contract["providers.list"].input,
  "providers/ProviderRef": contract["providers.remove"].input,
  "providers/ProvidersGetInput": contract["providers.get"].input,
  "providers/SetDefaultModelInput": contract["providers.setDefaultModel"].input,
  "providers/ProvidersRemoveOutput": contract["providers.remove"].output,
  "providers/ProbeOutcomeDto": ProbeOutcomeDto,
  "providers/ProbeRequest": contract["providers.probe"].input,
  "providers/ProbeResultDto": contract["providers.probe"].output,
  "browser/BrowserBounds": BrowserBounds,
  "browser/BrowserCommand": BrowserCommand,
  "accounts/AccountDto": AccountDto,
  "accounts/AccountLinkInput": contract["accounts.link"].input,
  "accounts/AccountRef": contract["accounts.refresh"].input,
  "accounts/AccountLinkResult": contract["accounts.link"].output,
  "accounts/AccountsCancelLinkOutput": contract["accounts.cancelLink"].output,
  "accounts/AccountsUnlinkOutput": contract["accounts.unlink"].output,
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

  it("holds the secret type registry exactly as secrets.types returns it", () => {
    const [registry] = cases(join(root, "secrets/SecretsTypesOutput.valid.json"));
    expect(registry?.value).toEqual(SECRET_TYPE_REGISTRY);
  });
});
