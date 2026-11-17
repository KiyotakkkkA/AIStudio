import { observer } from "mobx-react-lite";
import {
  mdiBookOpenPageVariantOutline,
  mdiBrain,
  mdiChatOutline,
  mdiChevronRight,
  mdiCogOutline,
  mdiConnection,
  mdiDatabaseOutline,
  mdiSourceBranch,
} from "@mdi/js";
import type { TransferCategory } from "@zvs/shared";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import { storageSize } from "./storageCopy";
import {
  DATA_CATEGORY_COPY,
  DATA_ALREADY_IMPORTED,
  DATA_CONNECTION_DEPENDENCY,
  DATA_CONTENTS,
  DATA_SELECTED,
  dataObjectCount,
} from "./dataTransferCopy";
import type DataTransferStore from "./DataTransferStore";

const CATEGORY_ICONS: Record<TransferCategory, string> = {
  conversations: mdiChatOutline,
  stores: mdiDatabaseOutline,
  scenarios: mdiSourceBranch,
  skills: mdiBookOpenPageVariantOutline,
  memory: mdiBrain,
  connections: mdiConnection,
  settings: mdiCogOutline,
};

export default observer(function DataTransferContents({
  store,
}: {
  readonly store: DataTransferStore;
}) {
  return (
    <PreferenceContainer title={DATA_CONTENTS}>
      <div className="divide-y divide-line">
        {store.categories.map((category) => {
          const item = store.preview?.categories.find((item) => item.category === category);
          const copy = DATA_CATEGORY_COPY[category];
          return (
            <div
              key={category}
              className="grid grid-cols-[20px_minmax(0,1fr)_auto] items-start gap-x-3 gap-y-3 py-4"
            >
              <Icon path={CATEGORY_ICONS[category]} size={18} className="mt-0.5 text-main-400" />
              <div className="min-w-0">
                <p className="m-0 text-[13px] font-medium text-main-100">{copy.label}</p>
                <p className="m-0 mt-1 text-[12px] text-main-500">{copy.hint}</p>
                {store.alreadyImported(category) ? (
                  <p className="m-0 mt-1 text-[12px] text-main-400">{DATA_ALREADY_IMPORTED}</p>
                ) : null}
                <p className="m-0 mt-1 font-mono text-[11px] text-main-400 tabular-nums">
                  {dataObjectCount(item?.count ?? 0)} · {storageSize(item?.bytes ?? 0)}
                </p>
              </div>
              <Toggle
                label={copy.label}
                checked={store.selected.has(category)}
                disabled={
                  store.busy ||
                  store.imported ||
                  store.alreadyImported(category) ||
                  item?.count === 0
                }
                onChange={(checked) => store.toggle(category, checked)}
              />
              {item?.names === undefined || item.names.length === 0 ? null : (
                <details className="group/objects col-span-2 col-start-2 min-w-0 overflow-hidden rounded-control border border-line bg-main-900 text-[12px] text-main-400">
                  <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2 select-none hover:bg-main-800 hover:text-main-200 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent-medium [&::-webkit-details-marker]:hidden">
                    <Icon
                      path={mdiChevronRight}
                      size={14}
                      className="shrink-0 group-open/objects:rotate-90 anim-on:transition-transform"
                    />
                    <span>Список объектов</span>
                    <span className="ml-auto font-mono text-[11px] text-main-500 tabular-nums">
                      {item.count.toLocaleString("ru-RU")}
                    </span>
                  </summary>
                  <ul className="m-0 max-h-64 list-none divide-y divide-line overflow-y-auto border-t border-line px-3 py-1">
                    {item.names.map((name, index) => (
                      <li key={index} className="py-2 text-main-300 wrap-break-word">
                        {name}
                      </li>
                    ))}
                    {item.count > item.names.length ? (
                      <li className="py-2 text-main-500">
                        Ещё {dataObjectCount(item.count - item.names.length)}
                      </li>
                    ) : null}
                  </ul>
                </details>
              )}
            </div>
          );
        })}
      </div>
      {store.hasDependencies ? (
        <p className="m-0 border-t border-line pt-3 text-[12px] text-main-500">
          {DATA_CONNECTION_DEPENDENCY}
        </p>
      ) : null}
      <div
        aria-live="polite"
        className="mt-3 mb-4 flex items-center justify-between rounded-control bg-main-900 px-3 py-2.5 text-[12px] text-main-300"
      >
        <span>
          {DATA_SELECTED}: {dataObjectCount(store.selectedCount)}
        </span>
        <span className="font-mono tabular-nums">{storageSize(store.selectedBytes)}</span>
      </div>
    </PreferenceContainer>
  );
});
