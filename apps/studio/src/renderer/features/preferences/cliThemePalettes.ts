import { CLI_THEMES, type CliTheme } from "@zvs/shared";
import { CLI_THEME_NAMES } from "./cliCopy";

export interface CliThemeSwatch {
  readonly id: CliTheme;
  readonly name: string;
  readonly frame: string;
  readonly panel: string;
  readonly user: string;
  readonly ink: string;
  readonly muted: string;
  readonly faint: string;
  readonly accent: string;
  readonly ok: string;
  readonly warn: string;
  readonly err: string;
}

const COLORS: Record<CliTheme, Omit<CliThemeSwatch, "id" | "name">> = {
  ember: {
    frame: "#1c1814",
    panel: "#221c17",
    user: "#2b221c",
    ink: "#ece3d9",
    muted: "#9c8e80",
    faint: "#5e5349",
    accent: "#f2914d",
    ok: "#9cc06b",
    warn: "#e8b84a",
    err: "#e86a58",
  },
  graphite: {
    frame: "#181a1e",
    panel: "#1c1f24",
    user: "#22252b",
    ink: "#e2e5ea",
    muted: "#8e95a0",
    faint: "#555b64",
    accent: "#7fa8f5",
    ok: "#8fc47a",
    warn: "#e5c06a",
    err: "#e86f6f",
  },
  moss: {
    frame: "#181b15",
    panel: "#1c2018",
    user: "#22271d",
    ink: "#e3e8da",
    muted: "#939c86",
    faint: "#58604e",
    accent: "#9cc86a",
    ok: "#7cc9a5",
    warn: "#e6bd58",
    err: "#e56f5c",
  },
  dusk: {
    frame: "#1b1721",
    panel: "#201b28",
    user: "#272130",
    ink: "#e8e2f0",
    muted: "#9a90a8",
    faint: "#5d556a",
    accent: "#b892f5",
    ok: "#8fc88a",
    warn: "#e8be62",
    err: "#ec7286",
  },
  frost: {
    frame: "#141a1c",
    panel: "#161f22",
    user: "#1b2629",
    ink: "#deeaec",
    muted: "#889a9e",
    faint: "#505f63",
    accent: "#6ecfde",
    ok: "#95cc7c",
    warn: "#e8c068",
    err: "#ec746c",
  },
};

export const CLI_THEME_SWATCHES: readonly CliThemeSwatch[] = CLI_THEMES.map((id) => ({
  id,
  name: CLI_THEME_NAMES[id],
  ...COLORS[id],
}));
