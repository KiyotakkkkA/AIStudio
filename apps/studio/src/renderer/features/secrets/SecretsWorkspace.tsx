import { mdiKeyOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import { useStore } from "../../stores/useStore";
import SecretConflictDialog from "./SecretConflictDialog";
import SecretForm from "./SecretForm";
import SecretList from "./SecretList";
import Alert from "../../ui/molecules/Alert";

function SecretsWorkspace() {
  const { secrets } = useStore();

  useEffect(() => {
    void secrets.load();
  }, [secrets]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {secrets.error === null ? null : (
        <Alert className="mx-6 mt-3" onDismiss={secrets.dismissError}>
          {secrets.error}
        </Alert>
      )}

      <div className="flex min-h-0 flex-1">
        <SecretList onSelect={secrets.requestSelect} onCreate={secrets.requestCreate} />

        {secrets.form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
            <EmptyState
              icon={mdiKeyOutline}
              title="Секрет не выбран"
              description="Выберите секрет в списке или создайте новый."
            />
          </div>
        ) : (
          <SecretForm
            vm={secrets.form}
            hint={secrets.selected?.hint ?? null}
            saving={secrets.saving}
            onSave={() => {
              void secrets.submit();
            }}
            onCancel={secrets.resetForm}
            onDelete={() => {
              void secrets.removeSelected();
            }}
          />
        )}
      </div>

      <SecretConflictDialog conflict={secrets.conflict} onClose={secrets.dismissConflict} />

      <ConfirmDialog
        open={secrets.pending !== null}
        title="Есть несохранённые изменения"
        confirmLabel="Не сохранять"
        confirmTone="danger"
        cancelLabel="Остаться"
        onConfirm={secrets.confirmPending}
        onCancel={secrets.cancelPending}
      >
        Изменения в форме не сохранены. Если продолжить, они будут потеряны.
      </ConfirmDialog>
    </div>
  );
}

export default observer(SecretsWorkspace);
