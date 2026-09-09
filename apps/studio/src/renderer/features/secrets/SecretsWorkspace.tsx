import { mdiKeyOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import useStore from "../../stores/useStore";
import SecretConflictDialog from "./SecretConflictDialog";
import SecretForm from "./SecretForm";
import SecretList from "./SecretList";

function SecretsWorkspace() {
  const { secrets } = useStore();

  useEffect(() => {
    void secrets.load();
  }, [secrets]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-[12px]">
      {secrets.error === null ? null : (
        <div
          role="alert"
          className="flex flex-none items-center gap-[12px] rounded-[6px] border border-err-border bg-main-800 px-[12px] py-[9px] text-[12px] text-err"
        >
          <span className="flex-1">{secrets.error}</span>
          <button type="button" className="text-main-400" onClick={secrets.dismissError}>
            Скрыть
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1 gap-[18px]">
        <SecretList onSelect={secrets.requestSelect} onCreate={secrets.requestCreate} />

        {secrets.form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center rounded-[10px] border border-main-700 bg-main-900">
            <EmptyState
              icon={mdiKeyOutline}
              title="Секрет не выбран"
              description="Выберите секрет слева или создайте новый — форма собирается по схеме выбранного типа."
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
