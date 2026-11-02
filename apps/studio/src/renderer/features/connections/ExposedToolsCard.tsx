import { mdiWrenchOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Chip from "../../ui/atoms/Chip";
import Toggle from "../../ui/atoms/Toggle";
import EmptyState from "../../ui/molecules/EmptyState";
import type ConnectionStore from "./ConnectionStore";
import Disclosure from "./Disclosure";
import TierPicker from "./TierPicker";

function ExposedToolsCard({ store }: { readonly store: ConnectionStore }) {
  const rows = store.toolRows;
  const isNew = store.form?.isNew === true;

  return (
    <section className="flex flex-none flex-col">
      <Disclosure
        label="Инструменты"
        defaultOpen
        aside={
          rows.length === 0 ? null : (
            <Chip>
              {String(store.enabledToolCount)} из {String(rows.length)} включено
            </Chip>
          )
        }
      >
        {rows.length === 0 ? (
          <div className="flex items-center justify-center py-4">
            <EmptyState
              icon={mdiWrenchOutline}
              title="Инструментов пока нет"
              description={
                isNew
                  ? "Сохраните сервер и проверьте связь."
                  : "Сервер ещё не отдавал список инструментов. Проверьте связь или подключите его."
              }
            />
          </div>
        ) : (
          <div className="flex flex-col pb-2">
            {rows.map(({ capability, tool }) => {
              const busy = tool !== null && store.isToolBusy(tool.id);
              const unavailable = tool === null || !tool.available;
              const muted = tool === null || !tool.enabled;
              return (
                <div
                  key={capability.id}
                  className={`flex items-center gap-2.75 py-2.25 ${muted ? "opacity-55" : ""}`}
                >
                  <Toggle
                    checked={tool?.enabled === true}
                    disabled={tool === null || busy}
                    label=""
                    onChange={(checked) => {
                      if (tool !== null) void store.setToolEnabled(tool.id, checked);
                    }}
                  />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="flex min-w-0 items-center gap-1.5">
                      <span className="truncate font-mono text-[12px] text-main-50">
                        {capability.toolKey ?? capability.name}
                      </span>
                      {capability.readOnly ? <Chip>read-only</Chip> : null}
                      {capability.destructive ? <Chip>destructive</Chip> : null}
                      {unavailable && tool !== null ? <Chip>недоступен</Chip> : null}
                    </span>
                    <span
                      className="truncate text-[11px] text-main-500"
                      title={capability.description}
                    >
                      {capability.description.length > 0
                        ? capability.description
                        : (capability.title ?? capability.name)}
                    </span>
                  </div>
                  {tool === null ? (
                    <Chip>не в реестре</Chip>
                  ) : (
                    <TierPicker
                      tier={tool.tier}
                      disabled={busy}
                      onChange={(tier) => {
                        void store.setToolTier(tool.id, tier);
                      }}
                    />
                  )}
                </div>
              );
            })}
          </div>
        )}
      </Disclosure>
    </section>
  );
}

export default observer(ExposedToolsCard);
