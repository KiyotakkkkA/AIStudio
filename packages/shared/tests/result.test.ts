import { expect, expectTypeOf, it } from "vitest";
import { z } from "zod";
import { AppErrorCode, Result, ok, err, isOk, SecretId } from "../src/index.js";

const schema = Result(z.object({ id: SecretId }));
const uuid = "01992958-f480-7000-8000-000000000001";

it("validates both branches and narrows their inferred types", () => {
  const success = schema.parse(ok({ id: SecretId.parse(uuid) }));
  const failure = schema.parse(err(AppErrorCode.NOT_FOUND, "Запись не найдена"));
  for (const result of [success, failure]) {
    if (isOk(result)) {
      expectTypeOf(result.data.id).toEqualTypeOf<SecretId>();
      expect(result.data.id).toBe(uuid);
    } else {
      expectTypeOf(result.error.code).toEqualTypeOf<AppErrorCode>();
      expect(result.error.code).toBe(AppErrorCode.NOT_FOUND);
    }
  }
  expect(failure).toEqual({
    ok: false,
    error: {
      code: AppErrorCode.NOT_FOUND,
      message: "Запись не найдена",
    },
  });
});

it("rejects unknown codes, malformed envelopes and invalid success data", () => {
  for (const value of [
    { ok: "true", data: { id: uuid } },
    { ok: true, data: { id: "invalid" } },
    { ok: false, error: { code: "BAD", message: "Ошибка" } },
    { ok: false, error: { code: AppErrorCode.UNKNOWN } },
  ]) {
    expect(schema.safeParse(value).success).toBe(false);
  }
});

it("keeps details JSON-shaped and survives a JSON round trip", () => {
  const result = err(AppErrorCode.VALIDATION_FAILED, "Проверьте данные", {
    fields: ["name"],
    nested: { count: 2, valid: false, value: null },
  });
  expect(schema.parse(JSON.parse(JSON.stringify(result)))).toEqual(result);
  for (const details of [
    null,
    new Date(),
    { value: new Map() },
    { value: undefined },
    { value: Infinity },
    { value: () => 1 },
  ]) {
    expect(
      schema.safeParse({
        ok: false,
        error: { code: AppErrorCode.UNKNOWN, message: "Ошибка", details },
      }).success,
    ).toBe(false);
  }
});
