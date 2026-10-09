import { mdiChevronDown } from "@mdi/js";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import type { ToolTier } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { TIER_HINTS, TIER_LABELS, TIER_TEXT } from "./connectionPresentation";

const TIERS: readonly ToolTier[] = ["auto", "ask", "off"];

export interface TierPickerProps {
  readonly tier: ToolTier;
  readonly disabled?: boolean;
  readonly onChange: (tier: ToolTier) => void;
}

export default function TierPicker({ tier, disabled = false, onChange }: TierPickerProps) {
  return (
    <Dropdown disabled={disabled} menuWidth={320} menuPlacement="bottom-right">
      <Dropdown.Trigger
        rounded=""
        disabled={disabled}
        title={TIER_HINTS[tier]}
        className={`h-5 w-auto gap-0.5 rounded-full border border-main-750 bg-main-800 px-2 font-mono text-[11px] font-medium ${TIER_TEXT[tier]} disabled:cursor-not-allowed disabled:opacity-50`}
        icon={<Icon path={mdiChevronDown} size={11} />}
      >
        {TIER_LABELS[tier]}
      </Dropdown.Trigger>
      <Dropdown.Menu
        role="menu"
        aria-label="Уровень разрешения"
        rounded=""
        className="rounded-card border-main-750"
      >
        {TIERS.map((entry) => (
          <Dropdown.Item
            key={entry}
            role="menuitemradio"
            aria-checked={entry === tier}
            rounded="rounded-lg"
            className="text-[13px] text-main-100"
            onClick={() => {
              if (entry !== tier) onChange(entry);
            }}
          >
            <span className="flex w-full items-baseline justify-between gap-2">
              <span className={`font-mono font-medium ${TIER_TEXT[entry]}`}>
                {TIER_LABELS[entry]}
              </span>
              <span className="text-[11px] text-main-400">{TIER_HINTS[entry]}</span>
            </span>
          </Dropdown.Item>
        ))}
      </Dropdown.Menu>
    </Dropdown>
  );
}
