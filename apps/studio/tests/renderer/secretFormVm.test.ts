import assert from "node:assert/strict";
import { test } from "vitest";
import {
  buildFieldsSchema,
  findSecretTypeSchema,
  SECRET_TYPE_REGISTRY,
  type SecretDto,
  type SecretId,
} from "@zvs/shared";
import { SecretFormVm } from "../../src/renderer/features/secrets/SecretFormVm.ts";

const ID = "0199aa11-1111-7111-8111-000000000042" as SecretId;

function ollamaSecret(overrides: Partial<SecretDto> = {}): SecretDto {
  return {
    id: ID,
    type: "ollama-cloud",
    name: "Ollama Cloud — личный",
    scope: "personal",
    hint: "osk_live_••••4f2a",
    tags: ["llm"],
    usageCount: 2,
    rotationStatus: "ok",
    rotatesAt: null,
    updatedAt: 1_700_000_000_000,
    fields: { baseUrl: "https://ollama.com/api", verifyTls: true },
    note: "Только для локальных экспериментов",
    ...overrides,
  };
}

test("switching the type swaps the credential fields and keeps name and scope", () => {
  const vm = new SecretFormVm(SECRET_TYPE_REGISTRY, ollamaSecret({ scope: "shared" }));
  vm.setField("organization", "zvs-lab");

  assert.deepEqual(
    vm.credentialFields.map((field) => field.key),
    ["baseUrl", "organization", "verifyTls"],
  );

  vm.setType("openrouter");

  assert.equal(vm.name, "Ollama Cloud — личный");
  assert.equal(vm.scope, "shared");
  assert.deepEqual(
    vm.credentialFields.map((field) => field.key),
    ["baseUrl", "referer"],
  );
  assert.equal(vm.fieldValues.organization, undefined);
  assert.equal(vm.fieldValues.baseUrl, "https://openrouter.ai/api/v1");
  assert.equal(vm.schemaChip, "openrouter@1");
});

test("defaults from the registry prefill a new form", () => {
  const vm = new SecretFormVm(SECRET_TYPE_REGISTRY);

  assert.equal(vm.type, "ollama-cloud");
  assert.equal(vm.isNew, true);
  assert.equal(vm.fieldValues.baseUrl, "https://ollama.com/api");
  assert.equal(vm.fieldValues.verifyTls, true);
  assert.equal(vm.dirty, false);
});

test("client validation agrees with the host schema on a malformed field", () => {
  const vm = new SecretFormVm(SECRET_TYPE_REGISTRY);
  vm.setName("Ollama");
  vm.setValue("osk_live_secret");
  vm.setField("baseUrl", "не ссылка");

  assert.equal(vm.validate(), false);
  assert.equal(vm.errorOf("fields.baseUrl") !== undefined, true);

  const schema = findSecretTypeSchema("ollama-cloud");
  if (schema === undefined) throw new Error("В реестре нет схемы ollama-cloud");
  assert.equal(buildFieldsSchema(schema).safeParse(vm.fieldsPayload()).success, false);

  vm.setField("baseUrl", "https://ollama.com/api");
  assert.equal(vm.validate(), true);
  assert.equal(buildFieldsSchema(schema).safeParse(vm.fieldsPayload()).success, true);
});

test("a new secret of a type with a required secret field demands a value", () => {
  const vm = new SecretFormVm(SECRET_TYPE_REGISTRY);
  vm.setName("Ollama");

  assert.equal(vm.valueRequired, true);
  assert.equal(vm.validate(), false);
  assert.equal(vm.errorOf("value") !== undefined, true);

  vm.setValue("osk_live_secret");
  assert.equal(vm.validate(), true);
  assert.equal(vm.toCreateInput().value, "osk_live_secret");
});

test("editing without touching the value omits it from the update payload", () => {
  const vm = new SecretFormVm(SECRET_TYPE_REGISTRY, ollamaSecret());
  vm.setName("Ollama Cloud — рабочий");

  assert.equal(vm.valueRequired, false);
  assert.equal(vm.validate(), true);

  const payload = vm.toUpdateInput();
  assert.equal(Object.hasOwn(payload, "value"), false);
  assert.equal(payload.name, "Ollama Cloud — рабочий");
  assert.deepEqual(payload.fields, { baseUrl: "https://ollama.com/api", verifyTls: true });

  vm.setValue("osk_live_new");
  assert.equal(vm.toUpdateInput().value, "osk_live_new");
});

test("the form reports dirty only after a real change", () => {
  const vm = new SecretFormVm(SECRET_TYPE_REGISTRY, ollamaSecret());
  assert.equal(vm.dirty, false);

  vm.setNote("Другая заметка");
  assert.equal(vm.dirty, true);
});
