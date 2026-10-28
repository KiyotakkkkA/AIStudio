import { observer } from "mobx-react-lite";
import { useEffect, useId, useState } from "react";
import { mdiCheck, mdiChevronDown } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import Reveal from "../../ui/motion/Reveal";
import { useStore } from "../../stores/useStore";
import { palettesFor, type Palette, type PaletteMode } from "../../app/palettes";
import { PALETTE_GROUPS, PALETTES_HINT, PALETTES_LABEL } from "./preferencesPresentation";
import { SETTING_IDS } from "./settingsIndex";

function PaletteCard({
  palette,
  selected,
  onSelect,
}: {
  readonly palette: Palette;
  readonly selected: boolean;
  readonly onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={`group flex min-w-0 flex-col overflow-hidden rounded-control border text-left anim-on:transition-colors ${
        selected ? "border-accent-dark" : "border-line hover:border-line-strong"
      }`}
    >
      <span
        aria-hidden="true"
        className="flex h-14 items-end p-2"
        style={{ background: palette.frame }}
      >
        <span
          className="flex w-full flex-col gap-1 rounded-[5px] p-1.5"
          style={{ background: palette.surface }}
        >
          <span className="h-1 w-3/4 rounded-full" style={{ background: palette.text }} />
          <span className="flex items-center gap-1">
            <span className="h-1 flex-1 rounded-full" style={{ background: palette.muted }} />
            <span className="size-2 rounded-full" style={{ background: palette.accent }} />
          </span>
        </span>
      </span>
      <span
        className={`flex items-center gap-1 px-2 py-1.5 text-[12px] ${
          selected ? "bg-accent-soft text-main-50" : "text-main-300 group-hover:text-main-100"
        }`}
      >
        <span className="min-w-0 flex-1 truncate">{palette.name}</span>
        {selected ? <Icon path={mdiCheck} size={13} className="shrink-0 text-accent-dark" /> : null}
      </span>
    </button>
  );
}

const PaletteGroup = observer(function PaletteGroup({
  mode,
  label,
}: {
  readonly mode: PaletteMode;
  readonly label: string;
}) {
  const { ui } = useStore();
  const labelId = useId();
  return (
    <div className="flex flex-col gap-2">
      <span id={labelId} className="text-[12px] font-medium text-main-300">
        {label}
      </span>
      <div
        role="radiogroup"
        aria-labelledby={labelId}
        className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-2"
      >
        {palettesFor(mode).map((palette) => (
          <PaletteCard
            key={palette.id}
            palette={palette}
            selected={ui.palettes[mode] === palette.id}
            onSelect={() => ui.setPalette(mode, palette.id)}
          />
        ))}
      </div>
    </div>
  );
});

const ThemePalettes = observer(function ThemePalettes() {
  const { ui } = useStore();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const targeted = ui.preferenceTarget === SETTING_IDS.palettes;

  useEffect(() => {
    if (targeted) setOpen(true);
  }, [targeted]);

  return (
    <div className="flex flex-col">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((value) => !value)}
        className="flex w-fit items-center gap-1 rounded-control text-[12px] text-main-400 hover:text-main-100 anim-on:transition-colors"
      >
        {PALETTES_LABEL}
        <Icon
          path={mdiChevronDown}
          size={14}
          className={`anim-on:transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      <Reveal show={open}>
        <div
          id={panelId}
          data-setting={SETTING_IDS.palettes}
          className={`flex flex-col gap-4 rounded-control pt-3 transition-colors duration-500 ${
            targeted ? "bg-accent-soft" : ""
          }`}
        >
          <span className="text-[12px] text-main-500">{PALETTES_HINT}</span>
          {PALETTE_GROUPS.map((group) => (
            <PaletteGroup key={group.mode} mode={group.mode} label={group.label} />
          ))}
        </div>
      </Reveal>
    </div>
  );
});

export default ThemePalettes;
