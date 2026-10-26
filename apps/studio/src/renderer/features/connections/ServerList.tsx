import { mdiPlus, mdiPowerPlugOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import EntityListPanel from "../../ui/molecules/EntityListPanel";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRow from "../../ui/molecules/ListRow";
import type ConnectionStore from "./ConnectionStore";
import { serverLine, STATE_LABELS, STATE_TONES } from "./connectionPresentation";
import { ListSkeleton } from "../../ui/organisms/PageSkeletons";
import { awaitingData } from "../../ui/molecules/LoadingSkeleton";

function ServerList({ store }: { readonly store: ConnectionStore }) {
  return (
    <EntityListPanel
      heading="MCP-серверы"
      search={{
        value: store.query,
        label: "Поиск серверов",
        placeholder: "Поиск серверов…",
        onChange: store.setQuery,
      }}
    >
      {awaitingData(store) ? <ListSkeleton /> : null}

      {store.isEmpty && store.form === null ? (
        <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-3">
          <EmptyState
            icon={mdiPowerPlugOutline}
            title="Серверов нет"
            description="Добавьте свой сервер или возьмите готовый из каталога."
          />
        </div>
      ) : null}

      {store.visibleServers.map((server) => {
        const state = store.stateOf(server);
        const tone = STATE_TONES[state];
        return (
          <div key={server.id} className={state === "disabled" ? "opacity-55" : undefined}>
            <ListRow
              selected={store.selectedId === server.id}
              onClick={() => {
                store.requestSelect(server.id);
              }}
            >
              <span className="flex min-w-0 items-center gap-2">
                <StatusDot tone={tone} title={STATE_LABELS[state]} />
                <span className="flex-1 truncate font-mono text-[12.5px] font-medium text-main-50">
                  {server.name}
                </span>
                <Chip>{server.transport}</Chip>
              </span>
              <span
                className={`truncate text-[11px] ${
                  state === "connected" || tone === "idle"
                    ? "text-main-400"
                    : STATUS_TONE_TEXT[tone]
                }`}
              >
                {serverLine(server, state)}
              </span>
            </ListRow>
          </div>
        );
      })}

      {store.form?.isNew === true ? (
        <ListRow selected>
          <span className="flex min-w-0 items-center gap-2">
            <StatusDot tone="idle" />
            <span className="flex-1 truncate font-mono text-[12.5px] font-medium text-main-50">
              {store.form.name.trim().length > 0 ? store.form.name : "новый сервер"}
            </span>
            <Chip>{store.form.transport}</Chip>
          </span>
          <span className="truncate text-[11px] text-main-400">Не сохранён</span>
        </ListRow>
      ) : null}

      <ListRow
        dashed
        onClick={store.openRegistry}
        leading={
          <span className="flex size-7.5 flex-none items-center justify-center rounded-control bg-main-700 text-main-400">
            <Icon path={mdiPlus} size={14} />
          </span>
        }
      >
        <span className="font-medium text-main-300">Добавить из каталога</span>
        <span className="truncate text-[11px] text-main-400">Готовые конфигурации серверов</span>
      </ListRow>
    </EntityListPanel>
  );
}

export default observer(ServerList);
