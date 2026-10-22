import { mdiDeleteOutline, mdiImageOutline, mdiLayersOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useEffect } from "react";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import { useAppNavigation } from "../../hooks/useAppNavigation";
import { ROUTES } from "../../app/routes";
import { useStore } from "../../stores/useStore";
import ConnectionCard from "./ConnectionCard";
import ModelGrid from "./ModelGrid";
import ModelSettingsCard from "./ModelSettingsCard";
import ProviderList from "./ProviderList";
import { modelsWord } from "./providerPresentation";

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
        <div
          role="alert"
          className="mx-6 mt-3 flex flex-none items-center gap-3 rounded-control bg-err-soft px-3 py-2.25 text-[12px] text-err"
        >
          <span className="flex-1">{providers.error}</span>
          <button type="button" className="text-main-400" onClick={providers.dismissError}>
            Скрыть
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <ProviderList onSelect={providers.requestSelect} onCreate={providers.requestCreate} />

        {form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
            <EmptyState
              icon={mdiLayersOutline}
              title="Подключение не выбрано"
              description="Выберите подключение слева или добавьте новое"
            />
          </div>
        ) : (
          <ScrollArea className="flex min-w-0 flex-1 flex-col gap-3.5 px-4.5 py-4">
            <div className="flex flex-none flex-col gap-3.5 xl:flex-row">
              <ConnectionCard
                vm={form}
                onManageSecrets={() => {
                  navigation.go(ROUTES.secrets);
                }}
              />
              <ModelSettingsCard vm={form} />
            </div>

            <ModelGrid />

            <div className="flex flex-none items-center gap-3 rounded-card border border-line bg-main-900 px-4 py-3">
              {form.isNew ? null : (
                <Button
                  key={form.providerId}
                  type="button"
                  tone="danger"
                  disabled={providers.removing}
                  needConfirm
                  modalSetup={{
                    title: "Удалить подключение?",
                    content: (
                      <>
                        «{form.name}» будет удалено вместе с кэшем моделей (
                        {modelsWord(providers.models.length)}
                        {providers.detail?.defaultModelId === null
                          ? ""
                          : ", включая выбранную по умолчанию"}
                        ). Сценарии и чаты, ссылающиеся на это подключение, останутся без модели.
                      </>
                    ),
                    tone: "danger",
                    confirmLabel: "Удалить",
                  }}
                  onClick={() => {
                    void providers.removeSelected();
                  }}
                >
                  <Icon path={mdiDeleteOutline} size={15} />
                  Удалить
                </Button>
              )}
              <Button
                type="button"
                tone="secondary"
                disabled={!form.dirty || providers.saving}
                needConfirm
                modalSetup={{
                  title: "Отменить?",
                  content: "Несохранённые изменения будут потеряны.",
                  confirmLabel: "Отменить",
                }}
                onClick={providers.resetForm}
              >
                Отменить
              </Button>
              <Button
                type="button"
                tone="primary"
                disabled={providers.saving}
                onClick={() => {
                  void providers.submit();
                }}
              >
                {providers.saving ? "Сохранение…" : "Сохранить подключение"}
              </Button>
            </div>
          </ScrollArea>
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
