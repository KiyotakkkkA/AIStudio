import { mdiDeleteOutline, mdiDotsHorizontal } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { useState } from "react";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { ActionMenu, ActionMenuItem } from "../../ui/molecules/ActionMenu";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import type ConnectionStore from "./ConnectionStore";

function ServerActionsMenu({ store }: { readonly store: ConnectionStore }) {
  const [confirmingRemove, setConfirmingRemove] = useState(false);
  const vm = store.form;
  if (vm === null) return null;
  return (
    <>
      {vm.isNew ? null : (
        <Dropdown menuWidth={220} menuPlacement="bottom-right">
          <IconDropdownButton
            label="Действия с сервером"
            tooltip="bottom-right"
            path={mdiDotsHorizontal}
          />
          <ActionMenu label="Действия с сервером">
            <ActionMenuItem
              icon={mdiDeleteOutline}
              danger
              disabled={store.removing}
              onClick={() => setConfirmingRemove(true)}
            >
              Удалить сервер
            </ActionMenuItem>
          </ActionMenu>
        </Dropdown>
      )}
      <ConfirmDialog
        open={confirmingRemove}
        title="Удалить сервер?"
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirmingRemove(false)}
        onConfirm={() => {
          setConfirmingRemove(false);
          void store.removeSelected();
        }}
      >
        «{vm.name}» будет остановлен и удалён. Его инструменты пропадут из реестра, а сценарии и
        навыки, которые на них ссылаются, перестанут их вызывать.
      </ConfirmDialog>
    </>
  );
}
export default observer(ServerActionsMenu);
