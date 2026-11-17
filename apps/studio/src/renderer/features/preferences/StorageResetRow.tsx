import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import TextInput from "../../ui/atoms/TextInput";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import SettingRow from "./SettingRow";
import type StorageStore from "./StorageStore";
import {
  STORAGE_CLEAR,
  STORAGE_RESET_LABEL,
  STORAGE_RESET_HINT,
  STORAGE_RESET_TITLE,
  STORAGE_RESET_CONSEQUENCE,
  STORAGE_RESET_INPUT,
  STORAGE_RESET_ID,
} from "./storageCopy";

export default observer(function StorageResetRow({ storage }: { readonly storage: StorageStore }) {
  return (
    <>
      <SettingRow
        id={STORAGE_RESET_ID}
        label={STORAGE_RESET_LABEL}
        hint={STORAGE_RESET_HINT}
        error={storage.resetChallenge === null ? (storage.resetError ?? undefined) : undefined}
      >
        <Button
          tone="danger-soft"
          disabled={storage.resetLoading || storage.resetting || storage.clearing}
          onClick={() => void storage.openReset()}
        >
          {STORAGE_CLEAR}
        </Button>
      </SettingRow>
      <ConfirmModal
        open={storage.resetChallenge !== null}
        title={STORAGE_RESET_TITLE}
        content={
          <div className="flex flex-col gap-3">
            <p className="m-0">{STORAGE_RESET_CONSEQUENCE}</p>
            <label className="flex flex-col gap-2">
              <span>
                Для подтверждения введите строку:{" "}
                <strong className="font-mono text-main-50">{storage.resetChallenge}</strong>
              </span>
              <TextInput
                aria-label={STORAGE_RESET_INPUT}
                value={storage.resetInput}
                onChange={(event) => storage.setResetInput(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                disabled={storage.resetting}
              />
            </label>
            {storage.resetError === null ? null : (
              <p role="alert" className="m-0 text-err">
                {storage.resetError}
              </p>
            )}
          </div>
        }
        confirmLabel={STORAGE_CLEAR}
        disabled={!storage.canReset}
        onCancel={storage.closeReset}
        onConfirm={() => void storage.reset()}
      />
    </>
  );
});
