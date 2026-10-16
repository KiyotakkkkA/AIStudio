import { expect, test } from "vitest";
import { Result, contract } from "@zvs/shared";
import { restoreEnvelope } from "../../src/renderer/app/tauriEnvelope.ts";

test("a void success from the Rust host satisfies the contract once the bridge restores data", () => {
  const envelope = Result(contract["runs.approve"].output);
  expect(envelope.safeParse({ ok: true }).success).toBe(false);
  expect(envelope.safeParse(restoreEnvelope({ ok: true })).success).toBe(true);
});

test("other envelopes pass through unchanged", () => {
  const data = { ok: true, data: { removed: 1 } };
  const failure = { ok: false, error: { code: "NOT_FOUND", message: "Запуск не найден" } };
  expect(restoreEnvelope(data)).toBe(data);
  expect(restoreEnvelope(failure)).toBe(failure);
  expect(restoreEnvelope(null)).toBeNull();
});
