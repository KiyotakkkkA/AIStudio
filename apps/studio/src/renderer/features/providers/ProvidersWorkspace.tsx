import { mdiImageOutline, mdiLayersOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useEffect } from "react";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import { useAppNavigation } from "../../hooks/useAppNavigation";
import { ROUTES } from "../../app/routes";
import { useStore } from "../../stores/useStore";
import ConnectionCard from "./ConnectionCard";
import ModelGrid from "./ModelGrid";
import ModelSettingsCard from "./ModelSettingsCard";
import ProviderList from "./ProviderList";
import ProviderActionsMenu from "./ProviderActionsMenu";
import ProviderControlActions from "./ProviderControlActions";
import Alert from "../../ui/molecules/Alert";
import { PaneSkeleton } from "../../ui/organisms/PageSkeletons";
import { awaitingData } from "../../ui/molecules/LoadingSkeleton";

function ProvidersWorkspace() {
  const { providers } = useStore();
  const navigation = useAppNavigation();
  const form = providers.form;

  useEffect(() => {
    void providers.load();
  }, [providers]);

  if (providers.loaded && providers.capability === "image" && !providers.capabilitySupported) {
    return (
      <div className="flex flex-1 items-center justify-center p-4.5">
        <EmptyState
          icon={mdiImageOutline}
          title="Провайдеров изображений пока нет"
          description="Генерация изображений не поддерживается ни одним из доступных семейств провайдеров."
        />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {providers.error === null ? null : (
        <Alert className="mx-6 mt-3" onDismiss={providers.dismissError}>
          {providers.error}
        </Alert>
      )}

      <div className="flex min-h-0 flex-1">
        <ProviderList onSelect={providers.requestSelect} onCreate={providers.requestCreate} />

        {form === null && awaitingData(providers) ? (
          <PaneSkeleton cards={3} />
        ) : form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
            <EmptyState
              icon={mdiLayersOutline}
              title="Подключение не выбрано"
              description="Выберите подключение слева или добавьте новое"
            />
          </div>
        ) : (
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <ScrollArea className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 px-5 py-4 lg:px-6">
              <div className="mx-auto flex w-full max-w-360 flex-none justify-end">
                <ProviderActionsMenu />
              </div>
              <div className="mx-auto grid w-full max-w-360 flex-none items-start gap-4 xl:grid-cols-[minmax(0,1.6fr)_minmax(18rem,1fr)]">
                <ConnectionCard
                  vm={form}
                  onManageSecrets={() => {
                    navigation.go(ROUTES.secrets);
                  }}
                />
                <ModelSettingsCard vm={form} />
              </div>

              <ModelGrid key={form.providerId ?? "new"} />
            </ScrollArea>

            <div className="flex-none px-5 pt-2 pb-4 lg:px-6">
              <ProviderControlActions />
            </div>
          </div>
        )}
      </div>

      <ConfirmDialog
        open={providers.pending !== null}
        title="Есть несохранённые изменения"
        confirmLabel="Не сохранять"
        confirmTone="danger"
        cancelLabel="Остаться"
        onConfirm={providers.confirmPending}
        onCancel={providers.cancelPending}
      >
        Изменения в форме подключения не сохранены. Если продолжить, они будут потеряны.
      </ConfirmDialog>
    </div>
  );
}

export default observer(ProvidersWorkspace);
