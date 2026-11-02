import { mdiPlus, mdiPowerPlugOutline, mdiViewGridPlusOutline } from "@mdi/js";
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
import Alert from "../../ui/molecules/Alert";
import { PaneSkeleton } from "../../ui/organisms/PageSkeletons";
import { InlineSkeleton, PAGE_LOADING, awaitingData } from "../../ui/molecules/LoadingSkeleton";

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
        subtitle={
          awaitingData(store) ? (
            <InlineSkeleton label={PAGE_LOADING} className="w-48" />
          ) : (
            `MCP-серверы · ${serversWord(store.servers.length)} · ${toolsWord(store.exposedToolCount)} доступно`
          )
        }
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
        <Alert className="mx-6 mt-3" onDismiss={store.dismissError}>
          {store.error}
        </Alert>
      )}

      <div className="flex min-h-0 flex-1">
        <ServerList store={store} />

        {form === null && awaitingData(store) ? (
          <PaneSkeleton />
        ) : form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
            <EmptyState
              icon={mdiPowerPlugOutline}
              title={store.isEmpty ? "MCP-серверов пока нет" : "Сервер не выбран"}
              description={
                store.isEmpty
                  ? "Добавьте сервер вручную или выберите готовый в каталоге."
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
