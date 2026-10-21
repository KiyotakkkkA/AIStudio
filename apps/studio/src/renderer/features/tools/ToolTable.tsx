import { mdiMagnify, mdiWrenchOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../../ui/atoms/Chip";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { effectiveTier } from "./toolFilters";
import type ToolStore from "./ToolStore";
import {
  formatLastUsed,
  sourceLabel,
  TIER_HINTS,
  TIER_LABELS,
  TIER_TEXT,
} from "./toolPresentation";

const HEAD = "text-[10px] font-semibold tracking-[0.07em] text-main-500 uppercase";

function ToolTable({ store }: { readonly store: ToolStore }) {
  const rows = store.visible;
  const groups = store.groups;

  return (
    <section aria-label="Реестр инструментов" className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex flex-none items-center gap-3 border-b border-line px-3.75 py-2.75">
        <span className={`flex-1 ${HEAD}`}>Инструмент</span>
        <span className={`w-26 flex-none ${HEAD}`}>Источник</span>
        <span className={`w-18.5 flex-none ${HEAD}`}>Разрешение</span>
        <span className={`w-19 flex-none text-right ${HEAD}`}>Вызывался</span>
      </div>

      {store.loading && store.tools.length === 0 ? (
        <div className="flex flex-col gap-2 p-3">
          <ListRowSkeleton />
          <ListRowSkeleton />
          <ListRowSkeleton />
        </div>
      ) : store.isEmpty ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiWrenchOutline}
            title="Инструментов пока нет"
            description="Встроенные инструменты регистрируются при запуске, инструменты MCP-серверов — после подключения сервера."
          />
        </div>
      ) : rows.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiMagnify}
            title="Ничего не найдено"
            description="Под выбранные фильтры не подходит ни один инструмент."
          />
        </div>
      ) : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col">
          <ul role="listbox" aria-label="Инструменты" className="m-0 flex list-none flex-col p-0">
            {rows.map((tool) => {
              const tier = effectiveTier(tool);
              const selected = store.selectedId === tool.id;
              const dimmed = tier === "off" || !tool.available;
              return (
                <li
                  key={tool.id}
                  role="option"
                  aria-selected={selected}
                  tabIndex={0}
                  onClick={() => {
                    void store.select(tool.id);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      void store.select(tool.id);
                    }
                  }}
                  className={`flex cursor-pointer items-center gap-3 border-b border-line px-3.75 py-2.5 outline-none focus-visible:bg-main-700 ${
                    selected
                      ? "bg-accent-soft shadow-[inset_2px_0_0_var(--color-accent-dark)]"
                      : "hover:bg-main-850"
                  }`}
                >
                  <div className={`flex min-w-0 flex-1 flex-col ${dimmed ? "opacity-55" : ""}`}>
                    <span className="flex min-w-0 items-center gap-1.5">
                      <span className="truncate font-mono text-[12.5px] font-medium text-main-50">
                        {tool.key}
                      </span>
                      {tool.available ? null : <Chip>недоступен</Chip>}
                    </span>
                    <span className="truncate text-[11px] text-main-400" title={tool.description}>
                      {tool.description.length > 0
                        ? tool.description
                        : tool.destructive
                          ? "Разрушающее действие."
                          : tool.displayName}
                    </span>
                  </div>
                  <span className={`flex w-26 flex-none ${dimmed ? "opacity-55" : ""}`}>
                    <Chip tone={tool.source === "builtin" ? "accent" : "neutral"}>
                      <span className="max-w-22 truncate">{sourceLabel(tool, groups)}</span>
                    </Chip>
                  </span>
                  <span className={`flex w-18.5 flex-none ${dimmed ? "opacity-55" : ""}`}>
                    <span
                      title={TIER_HINTS[tier]}
                      className={`inline-flex h-5 items-center rounded-full bg-main-800 px-2 font-mono text-[11px] font-medium ${TIER_TEXT[tier]}`}
                    >
                      {TIER_LABELS[tier]}
                    </span>
                  </span>
                  <span
                    className={`w-19 flex-none text-right font-mono text-[11px] text-main-500 ${dimmed ? "opacity-55" : ""}`}
                  >
                    {formatLastUsed(tool.lastUsedAt, store.now)}
                  </span>
                </li>
              );
            })}
          </ul>
        </ScrollArea>
      )}
    </section>
  );
}

export default observer(ToolTable);
