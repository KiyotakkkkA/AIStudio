import { describe, expect, expectTypeOf, it } from "vitest";
import { z } from "zod";
import {
  brandedId,
  createBrandedId,
  SecretId,
  ProviderId,
  Timestamp,
  timestampNow,
  PageRequest,
  Page,
  Sort,
  SortDirection,
} from "../src/index.js";

const uuid = "01992958-f480-7000-8000-000000000001";

describe("branded IDs", () => {
  it("validates UUIDs while preserving a plain string", () => {
    expect(createBrandedId(SecretId, uuid)).toBe(uuid);
    expect(brandedId("ExampleId").parse(uuid)).toBe(uuid);
    for (const value of ["", "secret-1", 42, null]) {
      expect(SecretId.safeParse(value).success).toBe(false);
    }
    expect(() => createBrandedId(SecretId, "invalid")).toThrow();
  });

  it("keeps IDs nominally distinct in typechecking", () => {
    const secret = createBrandedId(SecretId, uuid);
    expectTypeOf(secret).toEqualTypeOf<SecretId>();
    expectTypeOf<SecretId>().not.toExtend<ProviderId>();
    expectTypeOf<ProviderId>().not.toExtend<SecretId>();
    expectTypeOf<string>().not.toExtend<SecretId>();
  });
});

describe("timestamps", () => {
  it("accepts nonnegative epoch milliseconds and an injected clock", () => {
    expect(Timestamp.parse(0)).toBe(0);
    expect(timestampNow(() => 1788825600000)).toBe(1788825600000);
    expect(Timestamp.safeParse(timestampNow()).success).toBe(true);
  });
  it("rejects invalid clock values without coercion", () => {
    for (const value of [-1, 1.5, Infinity, NaN, "100", new Date(), null]) {
      expect(Timestamp.safeParse(value).success).toBe(false);
    }
    expect(() => timestampNow(() => -1)).toThrow();
  });
});

describe("paging and sorting", () => {
  it("validates requests and opaque cursors", () => {
    expect(PageRequest.parse({ limit: 20 })).toEqual({ limit: 20 });
    expect(PageRequest.parse({ limit: 20, cursor: "opaque" }).cursor).toBe("opaque");
    for (const request of [
      {},
      { limit: 0 },
      { limit: -1 },
      { limit: 1.5 },
      { limit: 1, cursor: null },
      { limit: 1, cursor: "" },
    ]) {
      expect(PageRequest.safeParse(request).success).toBe(false);
    }
  });
  it("validates every item and represents exhaustion by an absent cursor", () => {
    const schema = Page(z.object({ id: SecretId }));
    const page = schema.parse({ items: [{ id: uuid }], nextCursor: "next" });
    expectTypeOf(page.items[0]!.id).toEqualTypeOf<SecretId>();
    expect(schema.parse({ items: [] })).toEqual({ items: [] });
    expect(schema.safeParse({ items: [{ id: "bad" }] }).success).toBe(false);
    expect(schema.safeParse({ items: [], nextCursor: null }).success).toBe(false);
  });
  it("limits sorting to supported fields and directions", () => {
    const schema = Sort(z.enum(["name", "createdAt"]));
    expect(schema.parse({ field: "name", direction: "asc" })).toEqual({
      field: "name",
      direction: "asc",
    });
    expect(schema.safeParse({ field: "unknown", direction: "asc" }).success).toBe(false);
    expect(SortDirection.safeParse("ascending").success).toBe(false);
  });
});
