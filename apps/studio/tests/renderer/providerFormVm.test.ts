import assert from "node:assert/strict";
import { test } from "vitest";
import { CreateProviderInput, type ProbeResultDto } from "@zvs/shared";
import ProviderFormVm from "../../src/renderer/features/providers/ProviderFormVm.ts";
import { ACCOUNTS, ADAPTERS, provider, SECRET } from "./providerFixtures.ts";

function formFor(existing = false): ProviderFormVm {
  return new ProviderFormVm({
    adapters: ADAPTERS,
    accounts: ACCOUNTS,
    capability: "text",
    provider: existing ? provider() : null,
  });
}

const OK_PROBE: ProbeResultDto = {
  providerId: null,
  outcome: { kind: "ok", latencyMs: 412, live: true, models: [] },
  status: "ok",
  statusDetail: null,
  checkedAt: 1_700_000_000_000,
  provider: null,
};

test("editing the base URL clears a stale probe result", () => {
  const vm = formFor(true);
  vm.setProbeResult(OK_PROBE);
  assert.notEqual(vm.probeResult, null);

  vm.setBaseUrl("https://ollama.com/api/v2");

  assert.equal(vm.probeResult, null);
});

test("editing a sampling slider keeps a probe result — it says nothing about the wire", () => {
  const vm = formFor(true);
  vm.setProbeResult(OK_PROBE);

  vm.setSetting("temperature", 1.1);

  assert.notEqual(vm.probeResult, null);
});

test("switching the secret or the auth mode clears the probe result too", () => {
  const vm = formFor(true);
  vm.setProbeResult(OK_PROBE);
  vm.setSecretId(null);
  assert.equal(vm.probeResult, null);

  vm.setSecretId(SECRET);
  vm.setProbeResult(OK_PROBE);
  vm.setAdapter("qwen-web");
  assert.equal(vm.probeResult, null);
});

test("settings round-trip through the form untouched", () => {
  const vm = formFor(true);

  assert.equal(vm.settingOf("temperature"), 0.4);
  assert.equal(vm.isSettingExplicit("topK"), false);
  assert.deepEqual(vm.toUpdateInput().settings, { temperature: 0.4, maxOutputTokens: 2048 });

  vm.setSetting("topP", 0.8);
  assert.deepEqual(vm.toUpdateInput().settings, {
    temperature: 0.4,
    maxOutputTokens: 2048,
    topP: 0.8,
  });
});

test("required fields match the host contract", () => {
  const vm = formFor();

  assert.equal(vm.validate(), false);
  assert.equal(vm.errorOf("name"), "Укажите название подключения.");
  assert.equal(vm.errorOf("secretId"), "Выберите секрет с учётными данными.");

  vm.setName("Ollama");
  vm.setSecretId(SECRET);

  assert.equal(vm.validate(), true);
  assert.equal(CreateProviderInput.safeParse(vm.toCreateInput()).success, true);
});

test("a malformed base URL is refused before the host sees it", () => {
  const vm = formFor();
  vm.setName("Ollama");
  vm.setSecretId(SECRET);
  vm.setBaseUrl("ollama.com/api");

  assert.equal(vm.validate(), false);
  assert.equal(vm.errorOf("baseUrl"), "Нужен полный адрес, например https://host/api.");
});

test("an account-only family disables the API-key segment and switches the mode", () => {
  const vm = formFor();
  vm.setAdapter("qwen-web");

  assert.equal(vm.authMode, "account");
  assert.equal(
    vm.authModeDisabledReason("api"),
    "Семейство qwen-web работает только через привязанный аккаунт.",
  );
  assert.equal(vm.authModeDisabledReason("account"), null);

  vm.setAuthMode("api");
  assert.equal(vm.authMode, "account");
});

test("a family the host declares but has not wired up is refused", () => {
  // Every registered family is implemented today; the guard must still hold when one is not.
  const stub = ADAPTERS.map((entry) =>
    entry.family === "deepseek-web" ? { ...entry, implemented: false } : entry,
  );
  const vm = new ProviderFormVm({
    adapters: stub,
    accounts: ACCOUNTS,
    capability: "text",
    provider: null,
  });
  vm.setName("DeepSeek");
  vm.setSecretId(SECRET);
  vm.setAdapter("deepseek-web");

  assert.equal(vm.validate(), false);
  assert.equal(vm.errorOf("adapter"), "Семейство deepseek-web ещё не реализовано.");
});

test("tunables come from the descriptor, never from vendor guesswork", () => {
  const vm = formFor();

  assert.equal(vm.honours("temperature"), true);
  assert.equal(vm.honours("topK"), false);

  vm.setAdapter("qwen-web");
  assert.equal(vm.honours("temperature"), false);
});

test("capabilities the family cannot serve are refused with a reason", () => {
  const vm = formFor();
  vm.setAdapter("qwen-web");

  assert.equal(
    vm.capabilityDisabledReason("embedding"),
    "Семейство qwen-web не отдаёт эмбеддинги.",
  );
  vm.toggleCapability("embedding");
  assert.deepEqual(vm.capabilities, ["text"]);
});

test("switching the vendor moves the untouched base URL with it", () => {
  const vm = formFor();
  assert.equal(vm.baseUrl, "https://ollama.com/api");

  vm.setKind("openrouter");
  assert.equal(vm.baseUrl, "https://openrouter.ai/api/v1");

  vm.setBaseUrl("http://localhost:11434/v1");
  vm.setKind("mistral");
  assert.equal(vm.baseUrl, "http://localhost:11434/v1");
});

test("dirty tracking sees connection, name and settings edits", () => {
  const vm = formFor(true);
  assert.equal(vm.dirty, false);

  vm.setSetting("topK", 40);
  assert.equal(vm.dirty, true);
});
