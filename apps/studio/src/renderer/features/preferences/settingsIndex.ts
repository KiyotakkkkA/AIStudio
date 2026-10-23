import type { PreferenceSection } from "../../stores/UiStore";
import {
  APP_ROW_LABELS,
  DEVICE_ROW_LABELS,
  FONT_FAMILY_LABEL,
  FONT_SIZE_LABEL,
  GENERAL_ROWS,
  NOTIFICATION_KIND_ROWS,
  NOTIFICATIONS_ROW,
  RAIL_PICKER_LABEL,
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
  railPicker: "appearance.railPicker",
  fontFamily: "appearance.fontFamily",
  fontSize: "appearance.fontSize",
} as const;

const SEARCH_LIMIT = 8;

export function aboutSettingId(label: string): string {
  return `about.${label}`;
}

export const SETTING_ENTRIES: readonly SettingEntry[] = [
  ...GENERAL_ROWS.map((row) => ({ id: row.key, section: "general" as const, label: row.label })),
  { id: SETTING_IDS.theme, section: "appearance", label: THEME_LABEL },
  { id: SETTING_IDS.railPicker, section: "appearance", label: RAIL_PICKER_LABEL },
  { id: SETTING_IDS.fontFamily, section: "appearance", label: FONT_FAMILY_LABEL },
  { id: SETTING_IDS.fontSize, section: "appearance", label: FONT_SIZE_LABEL },
  { id: NOTIFICATIONS_ROW.key, section: "notifications", label: NOTIFICATIONS_ROW.label },
  ...NOTIFICATION_KIND_ROWS.map((row) => ({
    id: row.key,
    section: "notifications" as const,
    label: row.label,
    fallback: NOTIFICATIONS_ROW.key,
  })),
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
