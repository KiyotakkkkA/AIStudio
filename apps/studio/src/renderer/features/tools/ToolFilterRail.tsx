import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { ToolTier } from "@zvs/shared";
import FilterRailButton from "../../ui/molecules/FilterRailButton";
import { ALL_SOURCES } from "./toolFilters";
import type ToolStore from "./ToolStore";
import { groupTone, TIER_FILTER_LABELS, TIER_TONES } from "./toolPresentation";

const TIERS: readonly ToolTier[] = ["auto", "ask", "off"];

function ToolFilterRail({ store }: { readonly store: ToolStore }) {
  const sourceCounts = store.sourceCounts;
  const tierCounts = store.tierCounts;

  return (
    <aside
      aria-label="Фильтры инструментов"
      className="flex min-h-0 w-57.5 flex-none flex-col bg-main-800 p-2.5 pt-3.5"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-0.5">
        <p className="px-2 pb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Источники
        </p>
        <FilterRailButton
          active={store.source === ALL_SOURCES}
          label="Все инструменты"
          count={sourceCounts[ALL_SOURCES] ?? 0}
          onClick={() => {
            store.setSource(ALL_SOURCES);
          }}
        />
        {store.groups.map((group) => (
          <FilterRailButton
            key={group.key}
            active={store.source === group.key}
            label={group.label}
            count={sourceCounts[group.key] ?? 0}
            tone={groupTone(group)}
            onClick={() => {
              store.setSource(group.key);
            }}
          />
        ))}

        <p className="mt-4 px-2 pb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Разрешение
        </p>
        {TIERS.map((tier) => (
          <FilterRailButton
            key={tier}
            active={store.tier === tier}
            label={TIER_FILTER_LABELS[tier]}
            count={tierCounts[tier]}
            tone={TIER_TONES[tier]}
            onClick={() => {
              store.setTierFilter(tier);
            }}
          />
        ))}
      </ScrollArea>

      <div className="mt-3 flex-none rounded-card border border-line bg-main-900 px-2.75 py-2.5 text-[11.5px] leading-normal text-main-400">
        Разрешение, заданное здесь, — потолок. Навык или сценарий может его сузить, но не расширить.
      </div>
    </aside>
  );
}

export default observer(ToolFilterRail);
