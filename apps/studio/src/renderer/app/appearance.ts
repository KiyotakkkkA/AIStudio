import { autorun } from "mobx";
import { fontStack } from "../features/preferences/preferencesPresentation";
import type { UiStore } from "../stores/UiStore";
import { applyPalette } from "./palettes";

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";
const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";

function followQuery(query: string, apply: (matches: boolean) => void): () => void {
  if (typeof window.matchMedia !== "function") return () => undefined;
  const list = window.matchMedia(query);
  const sync = () => apply(list.matches);
  sync();
  list.addEventListener("change", sync);
  return () => list.removeEventListener("change", sync);
}

export function bindAppearance(ui: UiStore, root: HTMLElement = document.documentElement) {
  const disposers = [
    followQuery(DARK_SCHEME_QUERY, ui.setOsDark),
    followQuery(REDUCED_MOTION_QUERY, ui.setOsReducedMotion),
    autorun(() => {
      if (ui.theme === "system") delete root.dataset.theme;
      else root.dataset.theme = ui.theme;
    }),
    autorun(() => applyPalette(root, ui.palette)),
    autorun(() => {
      root.dataset.motion = ui.motion;
      root.dataset.effects = ui.effects;
    }),
    autorun(() => {
      const stack = fontStack(ui.fontFamily);
      if (stack === null) root.style.removeProperty("--font-sans");
      else root.style.setProperty("--font-sans", stack);
    }),
  ];
  return () => {
    for (const dispose of disposers) dispose();
  };
}
