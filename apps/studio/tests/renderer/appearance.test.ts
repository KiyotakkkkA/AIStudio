import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract, type Json } from "@zvs/shared";
import { bindAppearance } from "../../src/renderer/app/appearance.ts";
import { palettesFor } from "../../src/renderer/app/palettes.ts";
import { UiStore } from "../../src/renderer/stores/UiStore.ts";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";

function restoredStore(seed: Record<string, Json>) {
  const stored = new Map<string, Json>(Object.entries(seed));
  const clock = createFakeClock();
  const bridge = createFakeBridge<Contract>();
  bridge.handle("settings.get", ({ key }) => ({ key, value: stored.get(key) }));
  bridge.handle("settings.set", ({ key, value }) => {
    stored.set(key, value);
    return { key, value, updatedAt: clock.advance(1_000) };
  });
  return new UiStore(createIpcClient(contract, bridge));
}

const custom = palettesFor("dark").find((palette) => !palette.builtIn);

test("the restored theme, palette and font land on the root before anything renders", async () => {
  assert.ok(custom);
  const ui = restoredStore({
    "ui.theme": "dark",
    "ui.palette.dark": custom.id,
    "ui.font.family": "mono",
    "ui.fx.preset": "none",
  });
  const root = document.createElement("html");
  const dispose = bindAppearance(ui, root);

  await ui.restore();

  assert.equal(root.dataset.theme, "dark");
  assert.equal(root.dataset.palette, custom.id);
  assert.notEqual(root.style.length, 0);
  assert.notEqual(root.style.getPropertyValue("--font-sans"), "");
  assert.equal(root.dataset.motion, "none");
  assert.equal(root.dataset.effects, "none");
  dispose();
});

test("switching back to the system theme clears the override and the custom tokens", async () => {
  assert.ok(custom);
  const ui = restoredStore({ "ui.theme": "dark", "ui.palette.dark": custom.id });
  const root = document.createElement("html");
  const dispose = bindAppearance(ui, root);
  await ui.restore();

  ui.setTheme("system");
  ui.setOsDark(false);

  assert.equal(root.dataset.theme, undefined);
  assert.notEqual(root.dataset.palette, custom.id);
  assert.equal(root.style.length, 0);
  dispose();
});
