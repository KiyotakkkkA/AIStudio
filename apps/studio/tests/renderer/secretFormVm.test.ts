import assert from "node:assert/strict";
import { test } from "vitest";
import type { SecretDto, SecretId } from "@zvs/shared";
import { SecretFormVm } from "../../src/renderer/features/secrets/SecretFormVm.ts";

const ID = "0199aa11-1111-7111-8111-000000000042" as SecretId;

function storedSecret(overrides: Partial<SecretDto> = {}): SecretDto {
  return {
    id: ID,
    name: "Ollama — личный",
    scope: "personal",
    hint: "osk_live_••••4f2a",
    tags: ["llm"],
    usageCount: 2,
    rotationStatus: "ok",
    rotatesAt: null,
    updatedAt: 1_700_000_000_000,
    note: "Только для локальных экспериментов",
    ...overrides,
  };
}

test("a new form starts clean and sends only a name, scope, value and metadata", () => {
  const vm = new SecretFormVm();
  assert.equal(vm.isNew, true);
  assert.equal(vm.dirty, false);

  vm.setName("Бот Telegram");
  vm.setValue("123456:token");
  vm.setTags(["telegram"]);

  assert.equal(vm.validate(), true);
  assert.deepEqual(vm.toCreateInput(), {
    name: "Бот Telegram",
    scope: "personal",
    value: "123456:token",
    tags: ["telegram"],
  });
});

test("a new secret demands a value", () => {
  const vm = new SecretFormVm();
  vm.setName("Ollama");

  assert.equal(vm.valueRequired, true);
  assert.equal(vm.validate(), false);
  assert.equal(vm.errorOf("value") !== undefined, true);

  vm.setValue("osk_live_secret");
  assert.equal(vm.validate(), true);
  assert.equal(vm.toCreateInput().value, "osk_live_secret");
});

test("editing without touching the value omits it from the update payload", () => {
  const vm = new SecretFormVm(storedSecret());
  vm.setName("Ollama — рабочий");

  assert.equal(vm.valueRequired, false);
  assert.equal(vm.validate(), true);

  const payload = vm.toUpdateInput();
  assert.equal(Object.hasOwn(payload, "value"), false);
  assert.equal(payload.name, "Ollama — рабочий");

  vm.setValue("osk_live_new");
  assert.equal(vm.toUpdateInput().value, "osk_live_new");
});

test("the form reports dirty only after a real change", () => {
  const vm = new SecretFormVm(storedSecret());
  assert.equal(vm.dirty, false);

  vm.setNote("Другая заметка");
  assert.equal(vm.dirty, true);
});

test("a copy keeps the configuration and asks for the value again", () => {
  const vm = new SecretFormVm(storedSecret({ scope: "shared" }), true);
  assert.equal(vm.isNew, true);
  assert.equal(vm.name, "Ollama — личный (копия)");
  assert.equal(vm.scope, "shared");
  assert.equal(vm.dirty, true);
  assert.equal(vm.validate(), false);
});
