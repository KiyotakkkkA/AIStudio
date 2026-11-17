import { test, expect } from "@playwright/test";
import { temporaryDirectory } from "../test/helpers/paths.ts";
import { launchStudio, type Studio } from "./launch.ts";
import {
  STORAGE_CLEAR,
  STORAGE_RESET_TITLE,
  STORAGE_RESET_INPUT,
} from "../apps/studio/src/renderer/features/preferences/storageCopy.ts";

test("application reset requires the exact generated confirmation", async () => {
  const data = temporaryDirectory("studio-reset-validation-");
  let studio: Studio | undefined;
  try {
    studio = await launchStudio(data.path);
    const page = studio.page;
    await page.locator("aside").getByText("Параметры", { exact: true }).click();
    await page.getByRole("button", { name: "Данные", exact: true }).click();
    const row = page.locator('[data-setting="storage.reset"]');
    await row.getByRole("button", { name: STORAGE_CLEAR, exact: true }).click();
    const dialog = page.getByRole("dialog", { name: STORAGE_RESET_TITLE });
    await expect(dialog).toBeVisible();
    const code = (await dialog.locator("strong").innerText()).trim();
    expect(code).toMatch(/^[0-9a-f]{6}$/);
    const input = dialog.getByLabel(STORAGE_RESET_INPUT);
    const clear = dialog.getByRole("button", { name: STORAGE_CLEAR, exact: true });
    await expect(clear).toBeDisabled();
    await input.fill("wrong");
    await expect(clear).toBeDisabled();
    await input.fill(code);
    await expect(clear).toBeEnabled();
    await page.screenshot({ path: "test-results/storage-reset-modal.png", animations: "disabled" });
    await dialog.getByRole("button", { name: "Отмена", exact: true }).click();
    await expect(dialog).toBeHidden();
    await row.getByRole("button", { name: STORAGE_CLEAR, exact: true }).click();
    await expect(input).toHaveValue("");
    await expect(clear).toBeDisabled();
    await dialog.getByRole("button", { name: "Отмена", exact: true }).click();
  } finally {
    await studio?.close();
    data.dispose();
  }
});
