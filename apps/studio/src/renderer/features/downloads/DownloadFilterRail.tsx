import { observer } from "mobx-react-lite";
import { CatalogueItemState, DownloadItemKind } from "@zvs/shared";
import FilterRailButton from "../../ui/molecules/FilterRailButton";
import DiskBreakdown from "./DiskBreakdown";
import type DownloadStore from "./DownloadStore";
import { KIND_FILTER_LABELS, STATE_FILTER_LABELS, STATE_TONES } from "./downloadPresentation";

const STATUS_FILTERS = CatalogueItemState.options.filter((state) => state !== "available");

export default observer(function DownloadFilterRail({ store }: { readonly store: DownloadStore }) {
  return (
    <aside
      aria-label="Фильтры загрузок"
      className="flex w-54 flex-none flex-col gap-0.5 border-r border-main-750 p-2.5"
    >
      <p className="px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
        Каталог
      </p>
      <FilterRailButton
        active={store.kind === "all"}
        label="Всё"
        count={store.catalogue.length}
        onClick={() => store.setKind("all")}
      />
      {DownloadItemKind.options.map((kind) => (
        <FilterRailButton
          key={kind}
          active={store.kind === kind}
          label={KIND_FILTER_LABELS[kind]}
          count={store.kindCounts[kind]}
          onClick={() => store.setKind(kind)}
        />
      ))}

      <p className="mt-4 px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
        Состояние
      </p>
      {STATUS_FILTERS.map((state) => (
        <FilterRailButton
          key={state}
          active={store.state === state}
          label={STATE_FILTER_LABELS[state]}
          count={store.stateCounts[state]}
          tone={STATE_TONES[state]}
          onClick={() => store.setState(state)}
        />
      ))}

      <div className="flex-1" />
      <DiskBreakdown disk={store.disk} />
    </aside>
  );
});
