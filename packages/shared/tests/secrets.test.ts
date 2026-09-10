import { describe, expect, it } from "vitest";
import {
  buildFieldsSchema,
  configFieldsOf,
  CreateSecretInput,
  findSecretTypeSchema,
  isSecretTypeKey,
  schemaChip,
  SECRET_TYPE_KEYS,
  SECRET_TYPE_REGISTRY,
  SecretDto,
  secretFieldOf,
  SecretSummaryDto,
  SecretTypeSchema,
  UpdateSecretInput,
} from "../src/index.js";

const id = "01992958-f480-7000-8000-000000000001";

describe("the secret type registry", () => {
  it("holds exactly the declared keys, each parsing as a schema", () => {
    expect(SECRET_TYPE_REGISTRY.map((schema) => schema.key)).toEqual([...SECRET_TYPE_KEYS]);
    for (const schema of SECRET_TYPE_REGISTRY) {
      expect(SecretTypeSchema.parse(schema)).toEqual(schema);
    }
  });

  it("gives every entry exactly one secret field and unique field keys", () => {
    for (const schema of SECRET_TYPE_REGISTRY) {
      const secretFields = schema.fields.filter((field) => field.kind === "secret");
      expect(secretFields, `${schema.key} must declare one secret field`).toHaveLength(1);
      expect(secretFieldOf(schema)).toBe(secretFields[0]);
      const keys = schema.fields.map((field) => field.key);
      expect(new Set(keys).size, `${schema.key} has a duplicate field key`).toBe(keys.length);
    }
  });

  it("keeps type keys unique and looks them up", () => {
    const keys = SECRET_TYPE_REGISTRY.map((schema) => schema.key);
    expect(new Set(keys).size).toBe(keys.length);
    expect(findSecretTypeSchema("ollama")?.label).toBe("Ollama API key");
    expect(findSecretTypeSchema("nope")).toBeUndefined();
    expect(isSecretTypeKey("openrouter")).toBe(true);
    expect(isSecretTypeKey("nope")).toBe(false);
  });

  it("renders the chip the mockup shows", () => {
    expect(schemaChip(findSecretTypeSchema("ollama")!)).toBe("ollama@1");
  });

  it("declares only select fields with options", () => {
    for (const schema of SECRET_TYPE_REGISTRY) {
      for (const field of schema.fields) {
        if (field.kind !== "select") expect(field.options).toBeUndefined();
        else expect(field.options?.length).toBeGreaterThan(0);
      }
    }
  });
});

describe("buildFieldsSchema", () => {
  const ollama = findSecretTypeSchema("ollama")!;
  const withRequiredField: SecretTypeSchema = {
    key: "fixture",
    version: 1,
    label: "Fixture",
    fields: [
      { key: "apiKey", label: "API key", kind: "secret", required: true },
      { key: "url", label: "URL", kind: "url", required: true },
    ],
  };

  it("never asks for the secret field", () => {
    const parsed = buildFieldsSchema(ollama).parse({});
    expect(Object.keys(parsed)).not.toContain("apiKey");
    expect(configFieldsOf(ollama).map((field) => field.key)).toEqual([
      "baseUrl",
      "organization",
      "verifyTls",
    ]);
  });

  it("accepts a valid payload and fills optional defaults", () => {
    expect(buildFieldsSchema(ollama).parse({ organization: "zvs-lab" })).toEqual({
      baseUrl: "https://ollama.com/api",
      organization: "zvs-lab",
      verifyTls: true,
    });
  });

  it("rejects a missing required field", () => {
    expect(buildFieldsSchema(withRequiredField).safeParse({}).success).toBe(false);
    expect(
      buildFieldsSchema(withRequiredField).safeParse({ url: "http://127.0.0.1:6333" }).success,
    ).toBe(true);
  });

  it("rejects an undeclared field instead of storing it", () => {
    const result = buildFieldsSchema(ollama).safeParse({ organization: "x", sneaky: "value" });
    expect(result.success).toBe(false);
  });

  it("rejects a wrong type", () => {
    expect(buildFieldsSchema(ollama).safeParse({ verifyTls: "yes" }).success).toBe(false);
    expect(buildFieldsSchema(ollama).safeParse({ baseUrl: "not a url" }).success).toBe(false);
  });

  it("builds for every registry entry", () => {
    for (const schema of SECRET_TYPE_REGISTRY) {
      expect(() => buildFieldsSchema(schema)).not.toThrow();
    }
  });
});

describe("secret DTOs", () => {
  const summary = {
    id,
    type: "ollama" as const,
    name: "Ollama — personal",
    scope: "personal" as const,
    hint: "osk_live_…4f2a",
    tags: ["llm", "cloud"],
    usageCount: 2,
    rotationStatus: "due-soon" as const,
    rotatesAt: 1_700_000_000_000,
    updatedAt: 1_699_000_000_000,
  };

  it("keeps no value or cipher on the summary", () => {
    const parsed = SecretSummaryDto.parse(summary);
    expect(Object.keys(parsed).sort()).toEqual([
      "hint",
      "id",
      "name",
      "rotatesAt",
      "rotationStatus",
      "scope",
      "tags",
      "type",
      "updatedAt",
      "usageCount",
    ]);
  });

  it("strips a value that someone tries to smuggle through", () => {
    const parsed = SecretSummaryDto.parse({ ...summary, value: "osk_live_secret" });
    expect(Object.hasOwn(parsed, "value")).toBe(false);
  });

  it("adds only fields and note on the full DTO", () => {
    const parsed = SecretDto.parse({ ...summary, fields: { verifyTls: true }, note: null });
    expect(parsed.fields).toEqual({ verifyTls: true });
    expect(parsed.note).toBeNull();
  });

  it("defaults the optional halves of CreateSecretInput", () => {
    const parsed = CreateSecretInput.parse({
      type: "mistral",
      name: "Mistral",
      scope: "shared",
      value: "abc",
    });
    expect(parsed.fields).toEqual({});
    expect(parsed.tags).toEqual([]);
    expect(parsed.rotationDays).toBeUndefined();
  });

  it("treats an absent value on UpdateSecretInput as leave-the-credential-alone", () => {
    const parsed = UpdateSecretInput.parse({ id, name: "Renamed" });
    expect(Object.hasOwn(parsed, "value")).toBe(false);
    expect(UpdateSecretInput.safeParse({ id, value: "" }).success).toBe(false);
    expect(UpdateSecretInput.parse({ id, rotationDays: null }).rotationDays).toBeNull();
  });

  it("rejects an unknown secret type", () => {
    expect(
      CreateSecretInput.safeParse({ type: "aws", name: "x", scope: "personal", value: "y" })
        .success,
    ).toBe(false);
  });
});
