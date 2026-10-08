/**
 * The studio theme lives in settings under `ui.theme`. Mirroring it into Electron's
 * `nativeTheme.themeSource` makes `prefers-color-scheme` agree in every web contents —
 * the studio renderer and the browser chrome alike — without each one asking for it.
 */
export const THEME_SETTING_KEY = "ui.theme";

const THEME_SOURCES = ["system", "light", "dark"] as const;
type ThemeSource = (typeof THEME_SOURCES)[number];

export interface ThemeTarget {
  themeSource: ThemeSource;
}

export function applyThemeSource(value: unknown, target: ThemeTarget): void {
  if (typeof value !== "string") return;
  const source = THEME_SOURCES.find((item) => item === value);
  if (source !== undefined) target.themeSource = source;
}
