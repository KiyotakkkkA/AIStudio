import { mdiDeleteOutline, mdiPlus, mdiPowerPlugOutline, mdiViewGridPlusOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useEffect } from "react";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import { serversWord, toolsWord } from "./connectionPresentation";
import ExposedToolsCard from "./ExposedToolsCard";
import HealthRail from "./HealthRail";
import RegistryDialog from "./RegistryDialog";
import ServerList from "./ServerList";
import ServerSetupCard from "./ServerSetupCard";

function ConnectionsWorkspace() {
  const { connections: store } = useStore();
  const form = store.form;

  useEffect(() => {
    store.mount();
    return store.dispose;
  }, [store]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiPowerPlugOutline}
        title="Коннекторы"
        subtitle={`MCP-серверы · ${serversWord(store.servers.length)} · ${toolsWord(store.exposedToolCount)} доступно`}
        actions={
          <>
            <Button tone="secondary" onClick={store.openRegistry}>
              <Icon path={mdiViewGridPlusOutline} size={15} />
              Каталог
            </Button>
            <Button
              tone="primary"
              onClick={() => {
                store.requestCreate(null);
              }}
            >
              <Icon path={mdiPlus} size={15} />
              Добавить сервер
            </Button>
          </>
        }
      />

      {store.error === null ? null : (
        <div
          role="alert"
          className="mx-6 mt-3 flex flex-none items-center gap-3 rounded-control bg-err-soft px-3 py-2.25 text-[12px] text-err"
        >
          <span className="flex-1">{store.error}</span>
          <button type="button" className="text-main-400" onClick={store.dismissError}>
            Скрыть
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <ServerList store={store} />

        {form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
            <EmptyState
              icon={mdiPowerPlugOutline}
              title={store.isEmpty ? "MCP-серверов пока нет" : "Сервер не выбран"}
              description={
                store.isEmpty
                  ? "Добавьте сервер вручную или выберите готовый в каталоге — его инструменты появятся в реестре."
                  : "Выберите сервер слева или добавьте новый."
              }
              action={
                store.isEmpty ? (
                  <Button tone="primary" onClick={store.openRegistry}>
                    <Icon path={mdiViewGridPlusOutline} size={15} />
                    Открыть каталог
                  </Button>
                ) : undefined
              }
            />
          </div>
        ) : (
          <ScrollArea className="flex min-w-0 flex-1 flex-col gap-3.5 px-4.5 py-4">
            <ServerSetupCard store={store} vm={form} />
            <ExposedToolsCard store={store} />

            <div className="flex flex-none items-center gap-3 rounded-card border border-line bg-main-900 px-4 py-3">
              {form.isNew ? null : (
                <Button
                  key={form.serverId}
                  tone="danger"
                  disabled={store.removing}
                  needConfirm
                  modalSetup={{
                    title: "Удалить сервер?",
                    content: (
                      <>
                        «{form.name}» будет остановлен и удалён. Его инструменты пропадут из
                        реестра, а сценарии и навыки, которые на них ссылаются, перестанут их
                        вызывать.
                      </>
                    ),
                    tone: "danger",
                    confirmLabel: "Удалить",
                  }}
                  onClick={() => {
                    void store.removeSelected();
                  }}
                >
                  <Icon path={mdiDeleteOutline} size={15} />
                  Удалить
                </Button>
              )}
              <span className="flex-1" />
              <Button
                tone="secondary"
                disabled={(!form.isNew && !form.dirty) || store.saving}
                onClick={store.resetForm}
              >
                Отменить
              </Button>
              <Button
                tone="primary"
                disabled={store.saving || (!form.isNew && !form.dirty)}
                onClick={() => {
                  void store.submit();
                }}
              >
                {store.saving ? "Сохранение…" : "Сохранить"}
              </Button>
            </div>
          </ScrollArea>
        )}

        <HealthRail store={store} />
      </div>

      <RegistryDialog store={store} />

      <ConfirmDialog
        open={store.pending !== null}
        title="Есть несохранённые изменения"
        confirmLabel="Не сохранять"
        confirmTone="danger"
        cancelLabel="Остаться"
        onConfirm={store.confirmPending}
        onCancel={store.cancelPending}
      >
        Изменения в настройке сервера не сохранены. Если продолжить, они будут потеряны.
      </ConfirmDialog>
    </div>
  );
}

export default observer(ConnectionsWorkspace);
