import type { ITheme } from "@xterm/xterm";

const DARK_ANSI = {
  black: "#2a2521",
  red: "#e0705f",
  green: "#9cbf6e",
  yellow: "#e2b05c",
  blue: "#7ea6c4",
  magenta: "#c792b5",
  cyan: "#78b8b0",
  white: "#d8cec2",
  brightBlack: "#6f655b",
  brightRed: "#f08c7c",
  brightGreen: "#b5d68a",
  brightYellow: "#f2c77a",
  brightBlue: "#9cc0dc",
  brightMagenta: "#ddabcb",
  brightCyan: "#95d0c8",
  brightWhite: "#f7f1e8",
};

const LIGHT_ANSI = {
  black: "#2f2924",
  red: "#bf3a30",
  green: "#4d7a2a",
  yellow: "#94600b",
  blue: "#3f6c84",
  magenta: "#8e4a78",
  cyan: "#2f7a72",
  white: "#8a7e71",
  brightBlack: "#6c6258",
  brightRed: "#d4493e",
  brightGreen: "#5d8f35",
  brightYellow: "#a46b0d",
  brightBlue: "#4f84a0",
  brightMagenta: "#a65a8d",
  brightCyan: "#3a8f86",
  brightWhite: "#453d36",
};

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
    ...(dark ? DARK_ANSI : LIGHT_ANSI),
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
    attributeFilter: ["data-theme", "class", "style"],
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
