import type { ITheme } from "@xterm/xterm";
import { defaultPalette, findPalette } from "../../app/palettes";

function token(style: CSSStyleDeclaration, name: string, fallback: string): string {
  const value = style.getPropertyValue(name).trim();
  return value === "" ? fallback : value;
}

export function isDark(): boolean {
  return getComputedStyle(document.documentElement).colorScheme.includes("dark");
}

export function readTheme(): ITheme {
  const style = getComputedStyle(document.documentElement);
  const dark = isDark();
  return {
    ...(
      findPalette(document.documentElement.dataset.palette, dark ? "dark" : "light") ??
      defaultPalette(dark ? "dark" : "light")
    ).ansi,
    background: token(style, "--zvs-main-900", dark ? "#181513" : "#ffffff"),
    foreground: token(style, "--zvs-main-100", dark ? "#ebe3d8" : "#2f2924"),
    cursor: token(style, "--zvs-accent-dark", "#e57532"),
    cursorAccent: token(style, "--zvs-main-900", "#181513"),
    selectionBackground: token(style, "--zvs-accent-line", "#f0c9ad"),
    scrollbarSliderBackground: token(style, "--zvs-scrollbar", "rgba(120, 100, 80, 0.3)"),
    scrollbarSliderHoverBackground: token(style, "--zvs-scrollbar", "rgba(120, 100, 80, 0.3)"),
    scrollbarSliderActiveBackground: token(style, "--zvs-scrollbar", "rgba(120, 100, 80, 0.3)"),
  };
}

export function watchTheme(onChange: () => void): () => void {
  const attributes = new MutationObserver(onChange);
  attributes.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme", "data-palette", "class", "style"],
  });
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  media.addEventListener("change", onChange);
  return () => {
    attributes.disconnect();
    media.removeEventListener("change", onChange);
  };
}

export function adoptInlineStyles(container: HTMLElement): () => void {
  const sheet = new CSSStyleSheet();
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  const sync = () => {
    sheet.replaceSync(
      Array.from(container.querySelectorAll("style"))
        .map((element) => element.textContent)
        .join("\n"),
    );
  };
  const observer = new MutationObserver(sync);
  observer.observe(container, { childList: true, subtree: true, characterData: true });
  sync();
  return () => {
    observer.disconnect();
    document.adoptedStyleSheets = document.adoptedStyleSheets.filter((item) => item !== sheet);
  };
}
