import { describe, expect, it } from "vitest";
import { CreateSecretInput, SecretDto, SecretSummaryDto, UpdateSecretInput } from "../src/index.js";

const id = "01992958-f480-7000-8000-000000000001";

describe("secret DTOs", () => {
  const summary = {
    id,
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
      "updatedAt",
      "usageCount",
    ]);
  });

  it("strips a value that someone tries to smuggle through", () => {
    const parsed = SecretSummaryDto.parse({ ...summary, value: "osk_live_secret" });
    expect(Object.hasOwn(parsed, "value")).toBe(false);
  });

  it("adds only the note on the full DTO", () => {
    const parsed = SecretDto.parse({ ...summary, note: null });
    expect(parsed.note).toBeNull();
    expect(Object.hasOwn(SecretDto.parse({ ...summary, note: null, type: "ollama" }), "type")).toBe(
      false,
    );
  });

  it("defaults the optional halves of CreateSecretInput", () => {
    const parsed = CreateSecretInput.parse({
      name: "Mistral",
      scope: "shared",
      value: "abc",
    });
    expect(parsed.tags).toEqual([]);
    expect(parsed.rotationDays).toBeUndefined();
  });

  it("treats an absent value on UpdateSecretInput as leave-the-credential-alone", () => {
    const parsed = UpdateSecretInput.parse({ id, name: "Renamed" });
    expect(Object.hasOwn(parsed, "value")).toBe(false);
    expect(UpdateSecretInput.safeParse({ id, value: "" }).success).toBe(false);
    expect(UpdateSecretInput.parse({ id, rotationDays: null }).rotationDays).toBeNull();
  });
});
