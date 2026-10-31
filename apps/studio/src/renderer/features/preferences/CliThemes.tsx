import { observer } from "mobx-react-lite";
import { useId } from "react";
import { mdiCheck } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";
import { CLI_THEME_LABEL, CLI_THEME_PREVIEW_PROMPT } from "./cliCopy";
import { CLI_THEME_SWATCHES, type CliThemeSwatch } from "./cliThemePalettes";

function Bar({ color, width }: { readonly color: string; readonly width: string }) {
  return <span className="h-1 rounded-full" style={{ background: color, width }} />;
}

function TerminalPreview({ swatch }: { readonly swatch: CliThemeSwatch }) {
  return (
    <span
      aria-hidden="true"
      className="flex h-24 flex-col justify-between font-mono text-[9px] leading-none"
      style={{ background: swatch.frame }}
    >
      <span className="flex flex-col gap-1.5 px-2 pt-2">
        <span
          className="flex items-center gap-1.5 rounded-[3px] px-1 py-1"
          style={{ background: swatch.user }}
        >
          <Bar color={swatch.ink} width="55%" />
        </span>
        <span className="flex items-center gap-1.5 px-1">
          <span style={{ color: swatch.accent }}>◆</span>
          <Bar color={swatch.ink} width="70%" />
        </span>
        <span className="flex items-center gap-1.5 px-1">
          <span style={{ color: swatch.ok }}>✓</span>
          <Bar color={swatch.muted} width="40%" />
          <span className="ml-auto size-1.5 rounded-full" style={{ background: swatch.warn }} />
          <span className="size-1.5 rounded-full" style={{ background: swatch.err }} />
        </span>
      </span>
      <span className="flex items-center gap-1 px-2 py-1.5" style={{ background: swatch.panel }}>
        <span style={{ color: swatch.accent }}>›</span>
        <span className="truncate" style={{ color: swatch.faint }}>
          {CLI_THEME_PREVIEW_PROMPT}
        </span>
      </span>
    </span>
  );
}

function CliThemeCard({
  swatch,
  selected,
  disabled,
  onSelect,
}: {
  readonly swatch: CliThemeSwatch;
  readonly selected: boolean;
  readonly disabled: boolean;
  readonly onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      disabled={disabled}
      onClick={onSelect}
      className={`group flex min-w-0 flex-col overflow-hidden rounded-control border text-left disabled:opacity-60 anim-on:transition-colors ${
        selected ? "border-accent-dark" : "border-line hover:border-line-strong"
      }`}
    >
      <TerminalPreview swatch={swatch} />
      <span
        className={`flex items-center gap-1 px-2 py-1.5 text-[12px] ${
          selected ? "bg-accent-soft text-main-50" : "text-main-300 group-hover:text-main-100"
        }`}
      >
        <span className="size-2 shrink-0 rounded-full" style={{ background: swatch.accent }} />
        <span className="min-w-0 flex-1 truncate">{swatch.name}</span>
        {selected ? <Icon path={mdiCheck} size={13} className="shrink-0 text-accent-dark" /> : null}
      </span>
    </button>
  );
}

const CliThemes = observer(function CliThemes() {
  const { cli } = useStore();
  const labelId = useId();
  return (
    <div className="flex flex-col gap-2">
      <span id={labelId} className="sr-only">
        {CLI_THEME_LABEL}
      </span>
      <div
        role="radiogroup"
        aria-labelledby={labelId}
        className="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-2"
      >
        {CLI_THEME_SWATCHES.map((swatch) => (
          <CliThemeCard
            key={swatch.id}
            swatch={swatch}
            selected={cli.theme === swatch.id}
            disabled={cli.theme === null}
            onSelect={() => void cli.setTheme(swatch.id)}
          />
        ))}
      </div>
    </div>
  );
});

export default CliThemes;
