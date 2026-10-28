import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract } from "@zvs/shared";
import { DEFAULT_ROUTE, isRoutePath, type RoutePath } from "../app/routes";
import { FX_LEVELS, type FxLevel } from "../ui/motion/levels";
import {
  DEFAULT_PALETTES,
  defaultPalette,
  findPalette,
  type Palette,
  type PaletteMode,
} from "../app/palettes";

const RAIL_COLLAPSED_KEY = "ui.rail.collapsed";
const LAST_ROUTE_KEY = "ui.route.last";
const THEME_KEY = "ui.theme";
const RAIL_THEME_PICKER_KEY = "ui.rail.themepicker";
const FONT_FAMILY_KEY = "ui.font.family";
const FONT_SIZE_KEY = "ui.font.size";
const FX_PRESET_KEY = "ui.fx.preset";
const FX_MOTION_KEY = "ui.fx.motion";
const FX_EFFECTS_KEY = "ui.fx.effects";
const PALETTE_KEYS: Readonly<Record<PaletteMode, string>> = {
  dark: "ui.palette.dark",
  light: "ui.palette.light",
};

export const THEME_MODES = ["system", "light", "dark"] as const;
export type ThemeMode = (typeof THEME_MODES)[number];

function isThemeMode(value: unknown): value is ThemeMode {
  return typeof value === "string" && (THEME_MODES as readonly string[]).includes(value);
}

export const FONT_FAMILIES = ["onest", "system", "mono"] as const;
export type FontFamily = (typeof FONT_FAMILIES)[number];

export const FONT_SIZES = [12, 13, 14, 15, 16] as const;
export type FontSize = (typeof FONT_SIZES)[number];
export const DEFAULT_FONT_SIZE: FontSize = 13;

export { FX_LEVELS, type FxLevel };

export const FX_PRESETS = ["none", "reduced", "full", "custom"] as const;
export type FxPreset = (typeof FX_PRESETS)[number];

function isFxLevel(value: unknown): value is FxLevel {
  return typeof value === "string" && (FX_LEVELS as readonly string[]).includes(value);
}

function isFxPreset(value: unknown): value is FxPreset {
  return typeof value === "string" && (FX_PRESETS as readonly string[]).includes(value);
}

export const PREFERENCE_SECTIONS = [
  "general",
  "profile",
  "appearance",
  "notifications",
  "memory",
  "data",
  "about",
] as const;
export type PreferenceSection = (typeof PREFERENCE_SECTIONS)[number];

function isFontFamily(value: unknown): value is FontFamily {
  return typeof value === "string" && (FONT_FAMILIES as readonly string[]).includes(value);
}

function isFontSize(value: unknown): value is FontSize {
  return typeof value === "number" && (FONT_SIZES as readonly number[]).includes(value);
}

export class UiStore {
  railCollapsed = false;
  activeRoute: RoutePath = DEFAULT_ROUTE;
  theme: ThemeMode = "system";
  railThemePicker = true;
  fontFamily: FontFamily = "onest";
  fontSize: FontSize = DEFAULT_FONT_SIZE;
  fxPreset: FxPreset | null = null;
  customMotion: FxLevel = "full";
  customEffects: FxLevel = "full";
  osReducedMotion = false;
  osDark = false;
  palettes: Record<PaletteMode, string> = { ...DEFAULT_PALETTES };
  private customChosen = false;
  preferences: PreferenceSection | null = null;
  preferenceTarget: string | null = null;
  restored = false;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<UiStore, "ipc" | "customChosen">(
      this,
      { ipc: false, customChosen: false },
      { autoBind: true },
    );
  }

  get railWidth(): number {
    return this.railCollapsed ? 64 : 244;
  }

  get fxShown(): FxPreset {
    return this.fxPreset ?? (this.osReducedMotion ? "reduced" : "full");
  }

  get motion(): FxLevel {
    const preset = this.fxShown;
    return preset === "custom" ? this.customMotion : preset;
  }

  get effects(): FxLevel {
    if (this.fxPreset === null) return "full";
    return this.fxPreset === "custom" ? this.customEffects : this.fxPreset;
  }

  get themeShown(): PaletteMode {
    if (this.theme === "system") return this.osDark ? "dark" : "light";
    return this.theme;
  }

  get palette(): Palette {
    const mode = this.themeShown;
    return findPalette(this.palettes[mode], mode) ?? defaultPalette(mode);
  }

  async restore(): Promise<void> {
    const [
      collapsed,
      route,
      theme,
      picker,
      family,
      size,
      preset,
      motion,
      effects,
      darkPalette,
      lightPalette,
    ] = await Promise.all([
      this.read(RAIL_COLLAPSED_KEY),
      this.read(LAST_ROUTE_KEY),
      this.read(THEME_KEY),
      this.read(RAIL_THEME_PICKER_KEY),
      this.read(FONT_FAMILY_KEY),
      this.read(FONT_SIZE_KEY),
      this.read(FX_PRESET_KEY),
      this.read(FX_MOTION_KEY),
      this.read(FX_EFFECTS_KEY),
      this.read(PALETTE_KEYS.dark),
      this.read(PALETTE_KEYS.light),
    ]);
    runInAction(() => {
      if (isThemeMode(theme)) this.theme = theme;
      const dark = findPalette(darkPalette, "dark");
      if (dark !== undefined) this.palettes.dark = dark.id;
      const light = findPalette(lightPalette, "light");
      if (light !== undefined) this.palettes.light = light.id;
      if (typeof picker === "boolean") this.railThemePicker = picker;
      if (isFontFamily(family)) this.fontFamily = family;
      if (isFontSize(size)) this.fontSize = size;
      if (isFxPreset(preset)) this.fxPreset = preset;
      if (isFxLevel(motion)) this.customMotion = motion;
      if (isFxLevel(effects)) this.customEffects = effects;
      this.customChosen = isFxLevel(motion) || isFxLevel(effects);
      if (typeof collapsed === "boolean") this.railCollapsed = collapsed;
      if (isRoutePath(route)) this.activeRoute = route;
      this.restored = true;
    });
  }

  toggleRail(): void {
    this.setRailCollapsed(!this.railCollapsed);
  }

  setRailCollapsed(collapsed: boolean): void {
    if (this.railCollapsed === collapsed) return;
    this.railCollapsed = collapsed;
    void this.write(RAIL_COLLAPSED_KEY, collapsed);
  }

  setTheme(theme: ThemeMode): void {
    if (this.theme === theme) return;
    this.theme = theme;
    void this.write(THEME_KEY, theme);
  }

  setPalette(mode: PaletteMode, id: string): void {
    const palette = findPalette(id, mode);
    if (palette === undefined || this.palettes[mode] === palette.id) return;
    this.palettes[mode] = palette.id;
    void this.write(PALETTE_KEYS[mode], palette.id);
  }

  setOsDark(dark: boolean): void {
    this.osDark = dark;
  }

  setRailThemePicker(visible: boolean): void {
    if (this.railThemePicker === visible) return;
    this.railThemePicker = visible;
    void this.write(RAIL_THEME_PICKER_KEY, visible);
  }

  setFontFamily(family: FontFamily): void {
    if (this.fontFamily === family) return;
    this.fontFamily = family;
    void this.write(FONT_FAMILY_KEY, family);
  }

  setFontSize(size: FontSize): void {
    if (this.fontSize === size) return;
    this.fontSize = size;
    void this.write(FONT_SIZE_KEY, size);
  }

  setFxPreset(preset: FxPreset): void {
    if (this.fxPreset === preset) return;
    if (preset === "custom" && !this.customChosen) {
      this.customChosen = true;
      this.customMotion = this.motion;
      this.customEffects = this.effects;
      void this.write(FX_MOTION_KEY, this.customMotion);
      void this.write(FX_EFFECTS_KEY, this.customEffects);
    }
    this.fxPreset = preset;
    void this.write(FX_PRESET_KEY, preset);
  }

  setCustomMotion(level: FxLevel): void {
    this.customChosen = true;
    if (this.customMotion === level) return;
    this.customMotion = level;
    void this.write(FX_MOTION_KEY, level);
  }

  setCustomEffects(level: FxLevel): void {
    this.customChosen = true;
    if (this.customEffects === level) return;
    this.customEffects = level;
    void this.write(FX_EFFECTS_KEY, level);
  }

  setOsReducedMotion(reduced: boolean): void {
    this.osReducedMotion = reduced;
  }

  openPreferences(section: PreferenceSection = "appearance"): void {
    this.preferences = section;
  }

  revealPreference(section: PreferenceSection, target: string): void {
    this.preferences = section;
    this.preferenceTarget = target;
  }

  clearPreferenceTarget(): void {
    this.preferenceTarget = null;
  }

  closePreferences(): void {
    this.preferences = null;
    this.preferenceTarget = null;
  }

  setActiveRoute(route: string): void {
    if (!isRoutePath(route) || this.activeRoute === route) return;
    this.activeRoute = route;
    void this.write(LAST_ROUTE_KEY, route);
  }

  private async read(key: string): Promise<unknown> {
    try {
      const stored = await this.ipc.call("settings.get", { key });
      return stored.value;
    } catch {
      return undefined;
    }
  }

  private async write(key: string, value: boolean | string | number): Promise<void> {
    try {
      await this.ipc.call("settings.set", { key, value });
    } catch {
      return;
    }
  }
}
