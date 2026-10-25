import { mdiPlus } from "@mdi/js";
import { Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import type ConnectionStore from "./ConnectionStore";
import { joinArgs } from "./args";
import { SERVER_REGISTRY } from "./serverRegistry";

function RegistryDialog({ store }: { readonly store: ConnectionStore }) {
  return (
    <Modal
      open={store.registryOpen}
      onClose={store.closeRegistry}
      label="Каталог MCP-серверов"
      className="w-150 max-w-[92vw] overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
    >
      <div className="px-5 pt-5 pb-3">
        <h2 className="text-[15px] font-semibold tracking-[-0.01em] text-main-50">
          Каталог MCP-серверов
        </h2>
        <p className="mt-1 text-[13px] text-main-400">
          Готовая конфигурация открывается в форме — проверьте аргументы и секреты, затем сохраните.
        </p>
      </div>
      <ScrollArea className="flex max-h-[60vh] flex-col gap-2 px-5 pb-4">
        {SERVER_REGISTRY.map((preset) => (
          <button
            key={preset.id}
            type="button"
            className="flex w-full items-start gap-3 rounded-card border border-line bg-main-900 px-3.5 py-3 text-left hover:border-accent-line hover:bg-accent-soft"
            onClick={() => {
              store.requestCreate(preset);
            }}
          >
            <span className="flex min-w-0 flex-1 flex-col gap-1">
              <span className="flex items-center gap-2">
                <span className="text-[13px] font-semibold text-main-50">{preset.title}</span>
                <Chip>{preset.transport}</Chip>
              </span>
              <span className="text-[12px] text-main-400">{preset.description}</span>
              <span className="truncate font-mono text-[11px] text-main-500">
                {preset.transport === "stdio"
                  ? `${preset.command ?? ""} ${joinArgs(preset.args ?? [])}`
                  : preset.url}
              </span>
            </span>
            <Icon path={mdiPlus} size={16} className="mt-0.5 flex-none text-main-400" />
          </button>
        ))}
      </ScrollArea>
      <div className="flex justify-end gap-2 bg-main-850 px-5 py-3">
        <Button tone="secondary" onClick={store.closeRegistry}>
          Закрыть
        </Button>
        <Button
          tone="primary"
          onClick={() => {
            store.requestCreate(null);
          }}
        >
          Пустая форма
        </Button>
      </div>
    </Modal>
  );
}

export default observer(RegistryDialog);
