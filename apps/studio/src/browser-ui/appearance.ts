import type { BrowserAppearance } from "@zvs/shared";
import { applyPalette, defaultPalette, findPalette, type Palette } from "../renderer/app/palettes";

export function chromePalette(appearance: BrowserAppearance, osDark: boolean): Palette {
  const dark = appearance.theme === "dark" || (appearance.theme === "system" && osDark);
  const mode = dark ? "dark" : "light";
  const id = dark ? appearance.darkPalette : appearance.lightPalette;
  return findPalette(id, mode) ?? defaultPalette(mode);
}

export function applyChromeAppearance(
  appearance: BrowserAppearance,
  osDark: boolean,
  root: HTMLElement = document.documentElement,
): void {
  if (appearance.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = appearance.theme;
  applyPalette(root, chromePalette(appearance, osDark));
}
