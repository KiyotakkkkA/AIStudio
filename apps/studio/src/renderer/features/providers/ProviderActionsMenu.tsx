import { useState } from "react";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiDeleteOutline, mdiDotsHorizontal } from "@mdi/js";
import { useStore } from "../../stores/useStore";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { ActionMenu, ActionMenuItem } from "../../ui/molecules/ActionMenu";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";

export const PROVIDER_ACTIONS = "Действия с подключением";
export const DELETE_PROVIDER = "Удалить подключение";
export const DELETE_PROVIDER_TITLE = "Удалить подключение?";
export const DELETE_PROVIDER_ACCEPT = "Удалить";

function ProviderActionsMenu() {
  const { providers } = useStore();
  const [confirming, setConfirming] = useState(false);
  const form = providers.form;
  const disabled = form === null || form.isNew || providers.removing || providers.saving;

  return (
    <>
      <Dropdown menuWidth={220} menuPlacement="bottom-right" disabled={disabled}>
        <IconDropdownButton
          label={PROVIDER_ACTIONS}
          tooltip="bottom-right"
          path={mdiDotsHorizontal}
          size="md"
          disabled={disabled}
        />
        <ActionMenu label={PROVIDER_ACTIONS}>
          <ActionMenuItem
            icon={mdiDeleteOutline}
            danger
            disabled={disabled}
            onClick={() => setConfirming(true)}
          >
            {DELETE_PROVIDER}
          </ActionMenuItem>
        </ActionMenu>
      </Dropdown>
      <ConfirmDialog
        open={confirming}
        title={DELETE_PROVIDER_TITLE}
        confirmLabel={DELETE_PROVIDER_ACCEPT}
        confirmTone="danger"
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          if (!disabled) void providers.removeSelected();
        }}
      >
        Подключение «{form?.name}» и его список моделей будут удалены. Диалоги и сценарии,
        использующие это подключение, останутся без модели. Действие нельзя отменить.
      </ConfirmDialog>
    </>
  );
}

export default observer(ProviderActionsMenu);
