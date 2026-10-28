import type { PreferenceSection } from "../../stores/UiStore";
import { ENABLED_ROW, MODEL_ROW } from "../memory/memoryCopy";
import { ABOUT_ROW, STYLE_ROW, TOOL_CALLS_ROW } from "./profileCopy";
import { STORAGE_CLEAR_CACHE, STORAGE_CLEAR_ID } from "./storageCopy";
import {
  APP_ROW_LABELS,
  DEVICE_ROW_LABELS,
  FONT_FAMILY_LABEL,
  FONT_SIZE_LABEL,
  FX_EFFECTS_LABEL,
  FX_LABEL,
  FX_MOTION_LABEL,
  GENERAL_ROWS,
  NOTIFICATION_KIND_ROWS,
  NOTIFICATIONS_ROW,
  PALETTES_LABEL,
  RAIL_PICKER_LABEL,
  SECTION_LABELS,
  THEME_LABEL,
} from "./preferencesPresentation";

export interface SettingEntry {
  readonly id: string;
  readonly section: PreferenceSection;
  readonly label: string;
  readonly fallback?: string;
}

export interface SettingMatch {
  readonly entry: SettingEntry;
  readonly start: number;
  readonly end: number;
}

export const SETTING_IDS = {
  theme: "appearance.theme",
  palettes: "appearance.palettes",
  railPicker: "appearance.railPicker",
  fontFamily: "appearance.fontFamily",
  fontSize: "appearance.fontSize",
  fx: "appearance.fx",
  fxMotion: "appearance.fxMotion",
  fxEffects: "appearance.fxEffects",
  components: "components.runtimes",
} as const;

const SEARCH_LIMIT = 8;

export function aboutSettingId(label: string): string {
  return `about.${label}`;
}

export const SETTING_ENTRIES: readonly SettingEntry[] = [
  ...GENERAL_ROWS.map((row) => ({ id: row.key, section: "general" as const, label: row.label })),
  { id: TOOL_CALLS_ROW.key, section: "general", label: TOOL_CALLS_ROW.label },
  { id: STYLE_ROW.key, section: "profile", label: STYLE_ROW.label },
  { id: ABOUT_ROW.key, section: "profile", label: ABOUT_ROW.label },
  { id: SETTING_IDS.theme, section: "appearance", label: THEME_LABEL },
  {
    id: SETTING_IDS.palettes,
    section: "appearance",
    label: PALETTES_LABEL,
    fallback: SETTING_IDS.theme,
  },
  { id: SETTING_IDS.railPicker, section: "appearance", label: RAIL_PICKER_LABEL },
  { id: SETTING_IDS.fontFamily, section: "appearance", label: FONT_FAMILY_LABEL },
  { id: SETTING_IDS.fontSize, section: "appearance", label: FONT_SIZE_LABEL },
  { id: SETTING_IDS.fx, section: "appearance", label: FX_LABEL },
  {
    id: SETTING_IDS.fxMotion,
    section: "appearance",
    label: FX_MOTION_LABEL,
    fallback: SETTING_IDS.fx,
  },
  {
    id: SETTING_IDS.fxEffects,
    section: "appearance",
    label: FX_EFFECTS_LABEL,
    fallback: SETTING_IDS.fx,
  },
  { id: NOTIFICATIONS_ROW.key, section: "notifications", label: NOTIFICATIONS_ROW.label },
  ...NOTIFICATION_KIND_ROWS.map((row) => ({
    id: row.key,
    section: "notifications" as const,
    label: row.label,
    fallback: NOTIFICATIONS_ROW.key,
  })),
  { id: ENABLED_ROW.key, section: "memory", label: ENABLED_ROW.label },
  { id: MODEL_ROW.key, section: "memory", label: MODEL_ROW.label, fallback: ENABLED_ROW.key },
  { id: STORAGE_CLEAR_ID, section: "data", label: STORAGE_CLEAR_CACHE },
  { id: SETTING_IDS.components, section: "components", label: SECTION_LABELS.components },
  ...[...Object.values(APP_ROW_LABELS), ...Object.values(DEVICE_ROW_LABELS)].map((label) => ({
    id: aboutSettingId(label),
    section: "about" as const,
    label,
  })),
];

export function searchSettings(
  query: string,
  entries: readonly SettingEntry[] = SETTING_ENTRIES,
): SettingMatch[] {
  const needle = query.trim().toLocaleLowerCase("ru-RU");
  if (needle.length === 0) return [];
  return entries
    .flatMap((entry) => {
      const start = entry.label.toLocaleLowerCase("ru-RU").indexOf(needle);
      return start === -1 ? [] : [{ entry, start, end: start + needle.length }];
    })
    .sort((left, right) => Number(left.start !== 0) - Number(right.start !== 0))
    .slice(0, SEARCH_LIMIT);
}
