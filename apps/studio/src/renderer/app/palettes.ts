export type PaletteMode = "dark" | "light";

export interface AnsiColors {
  readonly black: string;
  readonly red: string;
  readonly green: string;
  readonly yellow: string;
  readonly blue: string;
  readonly magenta: string;
  readonly cyan: string;
  readonly white: string;
  readonly brightBlack: string;
  readonly brightRed: string;
  readonly brightGreen: string;
  readonly brightYellow: string;
  readonly brightBlue: string;
  readonly brightMagenta: string;
  readonly brightCyan: string;
  readonly brightWhite: string;
}

export interface Palette {
  readonly id: string;
  readonly name: string;
  readonly mode: PaletteMode;
  readonly builtIn: boolean;
  readonly frame: string;
  readonly surface: string;
  readonly raised: string;
  readonly hover: string;
  readonly strong: string;
  readonly border: string;
  readonly muted: string;
  readonly text: string;
  readonly ink: string;
  readonly accent: string;
  readonly ok: string;
  readonly warn: string;
  readonly err: string;
  readonly info: string;
  readonly ansi: AnsiColors;
}

function ansi(normal: readonly string[], bright: readonly string[]): AnsiColors {
  const [black, red, green, yellow, blue, magenta, cyan, white] = normal;
  const [
    brightBlack,
    brightRed,
    brightGreen,
    brightYellow,
    brightBlue,
    brightMagenta,
    brightCyan,
    brightWhite,
  ] = bright;
  return {
    black: black ?? "",
    red: red ?? "",
    green: green ?? "",
    yellow: yellow ?? "",
    blue: blue ?? "",
    magenta: magenta ?? "",
    cyan: cyan ?? "",
    white: white ?? "",
    brightBlack: brightBlack ?? "",
    brightRed: brightRed ?? "",
    brightGreen: brightGreen ?? "",
    brightYellow: brightYellow ?? "",
    brightBlue: brightBlue ?? "",
    brightMagenta: brightMagenta ?? "",
    brightCyan: brightCyan ?? "",
    brightWhite: brightWhite ?? "",
  };
}

const CUSTOM = { builtIn: false } as const;

export const PALETTES: readonly Palette[] = [
  {
    id: "zvs-dark",
    name: "ZVS",
    mode: "dark",
    builtIn: true,
    frame: "#0f0d0b",
    surface: "#181513",
    raised: "#1f1b18",
    hover: "#2a2521",
    strong: "#302a25",
    border: "#463e37",
    muted: "#8c8074",
    text: "#ebe3d8",
    ink: "#f7f1e8",
    accent: "#e57532",
    ok: "#a9be6a",
    warn: "#e5b350",
    err: "#ec6b5f",
    info: "#8fb3c4",
    ansi: ansi(
      ["#2a2521", "#e0705f", "#9cbf6e", "#e2b05c", "#7ea6c4", "#c792b5", "#78b8b0", "#d8cec2"],
      ["#6f655b", "#f08c7c", "#b5d68a", "#f2c77a", "#9cc0dc", "#ddabcb", "#95d0c8", "#f7f1e8"],
    ),
  },
  {
    id: "github-dark",
    name: "GitHub Dark",
    mode: "dark",
    ...CUSTOM,
    frame: "#010409",
    surface: "#0d1117",
    raised: "#161b22",
    hover: "#1c2128",
    strong: "#21262d",
    border: "#30363d",
    muted: "#8b949e",
    text: "#e6edf3",
    ink: "#f0f6fc",
    accent: "#4493f8",
    ok: "#3fb950",
    warn: "#d29922",
    err: "#f85149",
    info: "#58a6ff",
    ansi: ansi(
      ["#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf", "#b1bac4"],
      ["#6e7681", "#ffa198", "#56d364", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#ffffff"],
    ),
  },
  {
    id: "one-dark",
    name: "One Dark",
    mode: "dark",
    ...CUSTOM,
    frame: "#21252b",
    surface: "#282c34",
    raised: "#2c313a",
    hover: "#333842",
    strong: "#3a3f4b",
    border: "#4b5263",
    muted: "#7f848e",
    text: "#abb2bf",
    ink: "#d7dae0",
    accent: "#61afef",
    ok: "#98c379",
    warn: "#e5c07b",
    err: "#e06c75",
    info: "#56b6c2",
    ansi: ansi(
      ["#3f4451", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf"],
      ["#5c6370", "#ef7b85", "#a5d088", "#f0cb8a", "#73bcf5", "#d38be8", "#68c7d2", "#ffffff"],
    ),
  },
  {
    id: "dracula",
    name: "Dracula",
    mode: "dark",
    ...CUSTOM,
    frame: "#191a21",
    surface: "#282a36",
    raised: "#2e303e",
    hover: "#343746",
    strong: "#3c3f51",
    border: "#44475a",
    muted: "#7f8bb8",
    text: "#f8f8f2",
    ink: "#ffffff",
    accent: "#bd93f9",
    ok: "#50fa7b",
    warn: "#ffb86c",
    err: "#ff5555",
    info: "#8be9fd",
    ansi: ansi(
      ["#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2"],
      ["#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"],
    ),
  },
  {
    id: "nord",
    name: "Nord",
    mode: "dark",
    ...CUSTOM,
    frame: "#242933",
    surface: "#2e3440",
    raised: "#353b49",
    hover: "#3b4252",
    strong: "#434c5e",
    border: "#4c566a",
    muted: "#8891a5",
    text: "#e5e9f0",
    ink: "#eceff4",
    accent: "#88c0d0",
    ok: "#a3be8c",
    warn: "#ebcb8b",
    err: "#bf616a",
    info: "#81a1c1",
    ansi: ansi(
      ["#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0"],
      ["#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4"],
    ),
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    mode: "dark",
    ...CUSTOM,
    frame: "#11111b",
    surface: "#1e1e2e",
    raised: "#24243a",
    hover: "#2a2b3d",
    strong: "#313244",
    border: "#45475a",
    muted: "#7f849c",
    text: "#cdd6f4",
    ink: "#e2e8fb",
    accent: "#cba6f7",
    ok: "#a6e3a1",
    warn: "#f9e2af",
    err: "#f38ba8",
    info: "#89b4fa",
    ansi: ansi(
      ["#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de"],
      ["#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8"],
    ),
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    mode: "dark",
    ...CUSTOM,
    frame: "#16161e",
    surface: "#1a1b26",
    raised: "#1f2030",
    hover: "#24283b",
    strong: "#292e42",
    border: "#3b4261",
    muted: "#737aa2",
    text: "#c0caf5",
    ink: "#dfe3f7",
    accent: "#7aa2f7",
    ok: "#9ece6a",
    warn: "#e0af68",
    err: "#f7768e",
    info: "#7dcfff",
    ansi: ansi(
      ["#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6"],
      ["#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5"],
    ),
  },
  {
    id: "gruvbox-dark",
    name: "Gruvbox Dark",
    mode: "dark",
    ...CUSTOM,
    frame: "#1d2021",
    surface: "#282828",
    raised: "#32302f",
    hover: "#3c3836",
    strong: "#504945",
    border: "#665c54",
    muted: "#928374",
    text: "#ebdbb2",
    ink: "#fbf1c7",
    accent: "#fe8019",
    ok: "#b8bb26",
    warn: "#fabd2f",
    err: "#fb4934",
    info: "#83a598",
    ansi: ansi(
      ["#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984"],
      ["#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2"],
    ),
  },
  {
    id: "zvs-light",
    name: "ZVS",
    mode: "light",
    builtIn: true,
    frame: "#ece6dd",
    surface: "#ffffff",
    raised: "#faf7f3",
    hover: "#f2ede6",
    strong: "#ebe4da",
    border: "#ddd4c8",
    muted: "#8a7e71",
    text: "#2f2924",
    ink: "#1e1915",
    accent: "#b04c18",
    ok: "#4d7a2a",
    warn: "#a46b0d",
    err: "#bf3a30",
    info: "#3f6c84",
    ansi: ansi(
      ["#2f2924", "#bf3a30", "#4d7a2a", "#94600b", "#3f6c84", "#8e4a78", "#2f7a72", "#8a7e71"],
      ["#6c6258", "#d4493e", "#5d8f35", "#a46b0d", "#4f84a0", "#a65a8d", "#3a8f86", "#453d36"],
    ),
  },
  {
    id: "github-light",
    name: "GitHub Light",
    mode: "light",
    ...CUSTOM,
    frame: "#e9edf1",
    surface: "#ffffff",
    raised: "#f6f8fa",
    hover: "#eff2f5",
    strong: "#e6eaef",
    border: "#d0d7de",
    muted: "#6e7781",
    text: "#1f2328",
    ink: "#0d1117",
    accent: "#0969da",
    ok: "#1a7f37",
    warn: "#9a6700",
    err: "#cf222e",
    info: "#0969da",
    ansi: ansi(
      ["#24292f", "#cf222e", "#116329", "#4d2d00", "#0969da", "#8250df", "#1b7c83", "#6e7781"],
      ["#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#1f2328"],
    ),
  },
  {
    id: "one-light",
    name: "One Light",
    mode: "light",
    ...CUSTOM,
    frame: "#e5e5e6",
    surface: "#fafafa",
    raised: "#f3f3f3",
    hover: "#eaeaeb",
    strong: "#e0e0e1",
    border: "#d3d3d4",
    muted: "#7e8089",
    text: "#383a42",
    ink: "#202227",
    accent: "#4078f2",
    ok: "#50a14f",
    warn: "#c18401",
    err: "#e45649",
    info: "#0184bc",
    ansi: ansi(
      ["#383a42", "#e45649", "#50a14f", "#c18401", "#4078f2", "#a626a4", "#0184bc", "#a0a1a7"],
      ["#696c77", "#ca1243", "#3d8c3c", "#986801", "#3367d6", "#8a1f88", "#0997b3", "#383a42"],
    ),
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    mode: "light",
    ...CUSTOM,
    frame: "#dce0e8",
    surface: "#eff1f5",
    raised: "#e9ecf1",
    hover: "#e3e6ec",
    strong: "#d9dde5",
    border: "#ccd0da",
    muted: "#7c7f93",
    text: "#4c4f69",
    ink: "#303246",
    accent: "#8839ef",
    ok: "#40a02b",
    warn: "#df8e1d",
    err: "#d20f39",
    info: "#1e66f5",
    ansi: ansi(
      ["#5c5f77", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#acb0be"],
      ["#6c6f85", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#4c4f69"],
    ),
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    mode: "light",
    ...CUSTOM,
    frame: "#eee8d5",
    surface: "#fdf6e3",
    raised: "#f8f1dd",
    hover: "#f3ecd8",
    strong: "#ebe4cf",
    border: "#ddd6c1",
    muted: "#839496",
    text: "#586e75",
    ink: "#073642",
    accent: "#268bd2",
    ok: "#859900",
    warn: "#b58900",
    err: "#dc322f",
    info: "#2aa198",
    ansi: ansi(
      ["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#93a1a1"],
      ["#586e75", "#cb4b16", "#859900", "#b58900", "#268bd2", "#6c71c4", "#2aa198", "#073642"],
    ),
  },
  {
    id: "gruvbox-light",
    name: "Gruvbox Light",
    mode: "light",
    ...CUSTOM,
    frame: "#ebdbb2",
    surface: "#f9f5d7",
    raised: "#fbf1c7",
    hover: "#f2e5bc",
    strong: "#e5d5ad",
    border: "#d5c4a1",
    muted: "#7c6f64",
    text: "#3c3836",
    ink: "#282828",
    accent: "#af3a03",
    ok: "#79740e",
    warn: "#b57614",
    err: "#9d0006",
    info: "#076678",
    ansi: ansi(
      ["#3c3836", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#7c6f64"],
      ["#928374", "#9d0006", "#79740e", "#b57614", "#076678", "#8f3f71", "#427b58", "#3c3836"],
    ),
  },
];

export const DEFAULT_PALETTES: Readonly<Record<PaletteMode, string>> = {
  dark: "zvs-dark",
  light: "zvs-light",
};

export function palettesFor(mode: PaletteMode): readonly Palette[] {
  return PALETTES.filter((palette) => palette.mode === mode);
}

export function findPalette(id: unknown, mode?: PaletteMode): Palette | undefined {
  return PALETTES.find(
    (palette) => palette.id === id && (mode === undefined || palette.mode === mode),
  );
}

export function defaultPalette(mode: PaletteMode): Palette {
  const palette = findPalette(DEFAULT_PALETTES[mode], mode);
  if (palette === undefined) throw new Error(`No default ${mode} palette`);
  return palette;
}

function channels(hex: string): [number, number, number] {
  const value = Number.parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

function toHex(parts: readonly number[]): string {
  return `#${parts.map((part) => Math.round(part).toString(16).padStart(2, "0")).join("")}`;
}

export function mix(from: string, to: string, amount: number): string {
  const start = channels(from);
  const end = channels(to);
  return toHex(start.map((part, index) => part + ((end[index] ?? part) - part) * amount));
}

function alpha(hex: string, amount: number): string {
  const [red, green, blue] = channels(hex);
  return `rgb(${red} ${green} ${blue} / ${amount})`;
}

export function luminance(hex: string): number {
  const [red, green, blue] = channels(hex).map((part) => {
    const value = part / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (red ?? 0) + 0.7152 * (green ?? 0) + 0.0722 * (blue ?? 0);
}

export function contrast(first: string, second: string): number {
  const [light, dark] = [luminance(first), luminance(second)].sort((a, b) => b - a);
  return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
}

const DARK_SHADOWS = {
  "--zvs-shadow-card": "0 1px 2px rgb(0 0 0 / 0.25)",
  "--zvs-shadow-pop": "0 2px 6px rgb(0 0 0 / 0.3), 0 18px 44px rgb(0 0 0 / 0.45)",
  "--zvs-shadow-frame": "0 1px 2px rgb(0 0 0 / 0.3), 0 12px 32px rgb(0 0 0 / 0.25)",
};

const LIGHT_SHADOWS = {
  "--zvs-shadow-card": "0 1px 2px rgb(20 20 30 / 0.04), 0 8px 24px rgb(20 20 30 / 0.05)",
  "--zvs-shadow-pop": "0 2px 6px rgb(20 20 30 / 0.06), 0 18px 44px rgb(20 20 30 / 0.12)",
  "--zvs-shadow-frame": "0 1px 2px rgb(20 20 30 / 0.04), 0 10px 30px rgb(20 20 30 / 0.06)",
};

export function paletteTokens(palette: Palette): Readonly<Record<string, string>> {
  const dark = palette.mode === "dark";
  const toward = palette.ink;
  const onAccent =
    contrast(palette.accent, "#ffffff") >= contrast(palette.accent, palette.frame)
      ? "#ffffff"
      : palette.frame;
  const statuses = { ok: palette.ok, warn: palette.warn, err: palette.err, info: palette.info };
  const statusTokens = Object.fromEntries(
    Object.entries(statuses).flatMap(([name, color]) => [
      [`--zvs-${name}`, color],
      [`--zvs-${name}-soft`, alpha(color, dark ? 0.13 : 0.1)],
      [`--zvs-${name}-line`, alpha(color, dark ? 0.34 : 0.32)],
    ]),
  );
  return {
    "--zvs-main-950": palette.frame,
    "--zvs-main-900": palette.surface,
    "--zvs-main-850": mix(palette.surface, palette.raised, 0.5),
    "--zvs-main-800": palette.raised,
    "--zvs-main-750": palette.strong,
    "--zvs-main-700": palette.hover,
    "--zvs-main-600": palette.border,
    "--zvs-main-500": palette.muted,
    "--zvs-main-400": mix(palette.muted, palette.text, 0.3),
    "--zvs-main-300": mix(palette.muted, palette.text, 0.55),
    "--zvs-main-200": mix(palette.muted, palette.text, 0.8),
    "--zvs-main-100": palette.text,
    "--zvs-main-50": palette.ink,
    "--zvs-accent-light": mix(palette.accent, toward, 0.25),
    "--zvs-accent-medium": mix(palette.accent, toward, 0.1),
    "--zvs-accent-dark": palette.accent,
    "--zvs-accent-soft": alpha(palette.accent, dark ? 0.14 : 0.1),
    "--zvs-accent-line": alpha(palette.accent, dark ? 0.38 : 0.34),
    "--zvs-on-accent": onAccent,
    "--zvs-line": alpha(palette.text, dark ? 0.07 : 0.09),
    "--zvs-line-strong": alpha(palette.text, dark ? 0.14 : 0.17),
    ...statusTokens,
    ...(dark ? DARK_SHADOWS : LIGHT_SHADOWS),
    "--zvs-shimmer": alpha(palette.text, dark ? 0.06 : 0.07),
    "--zvs-scrollbar": alpha(palette.muted, dark ? 0.3 : 0.35),
  };
}

export const PALETTE_TOKEN_NAMES: readonly string[] = Object.keys(
  paletteTokens(defaultPalette("dark")),
);
