import assert from "node:assert/strict";
import { test } from "vitest";
import {
  DEFAULT_PALETTES,
  PALETTE_TOKEN_NAMES,
  PALETTES,
  contrast,
  defaultPalette,
  findPalette,
  luminance,
  paletteTokens,
  palettesFor,
} from "../../src/renderer/app/palettes.ts";

const HEX = /^#[0-9a-f]{6}$/;
const COLOR_FIELDS = [
  "frame",
  "surface",
  "raised",
  "hover",
  "strong",
  "border",
  "muted",
  "text",
  "ink",
  "accent",
  "ok",
  "warn",
  "err",
  "info",
] as const;

test("each mode offers the editor classics with the built-in ZVS palette first", () => {
  assert.deepEqual(
    palettesFor("dark").map((palette) => palette.name),
    [
      "ZVS",
      "GitHub Dark",
      "One Dark",
      "Dracula",
      "Nord",
      "Catppuccin Mocha",
      "Tokyo Night",
      "Gruvbox Dark",
    ],
  );
  assert.deepEqual(
    palettesFor("light").map((palette) => palette.name),
    ["ZVS", "GitHub Light", "One Light", "Catppuccin Latte", "Solarized Light", "Gruvbox Light"],
  );
  assert.equal(new Set(PALETTES.map((palette) => palette.id)).size, PALETTES.length);
  assert.equal(defaultPalette("dark").id, DEFAULT_PALETTES.dark);
  assert.equal(defaultPalette("light").id, DEFAULT_PALETTES.light);
  assert.deepEqual(
    PALETTES.filter((palette) => palette.builtIn).map((palette) => palette.id),
    ["zvs-dark", "zvs-light"],
  );
});

test("a palette is found only under its own mode", () => {
  assert.equal(findPalette("dracula", "dark")?.name, "Dracula");
  assert.equal(findPalette("dracula", "light"), undefined);
  assert.equal(findPalette("dracula")?.mode, "dark");
  assert.equal(findPalette(42), undefined);
});

test("every palette is complete hex and reads in its own mode", () => {
  for (const palette of PALETTES) {
    for (const field of COLOR_FIELDS) {
      assert.match(palette[field], HEX, `${palette.id}.${field}`);
    }
    for (const [name, color] of Object.entries(palette.ansi)) {
      assert.match(color, HEX, `${palette.id}.ansi.${name}`);
    }
    const surfaceDarker = luminance(palette.surface) < luminance(palette.text);
    assert.equal(surfaceDarker, palette.mode === "dark", palette.id);
    assert.ok(contrast(palette.text, palette.surface) >= 4.5, `${palette.id} text`);
    assert.ok(contrast(palette.muted, palette.surface) >= 2.8, `${palette.id} muted`);
  }
});

test("every custom palette defines the whole token set with readable accent text", () => {
  for (const palette of PALETTES) {
    const tokens = paletteTokens(palette);
    assert.deepEqual(Object.keys(tokens).sort(), [...PALETTE_TOKEN_NAMES].sort(), palette.id);
    assert.equal(tokens["--zvs-main-900"], palette.surface);
    assert.equal(tokens["--zvs-main-100"], palette.text);
    const onAccent = tokens["--zvs-on-accent"] ?? "";
    assert.ok(contrast(onAccent, palette.accent) >= 3, `${palette.id} on-accent`);
  }
});
