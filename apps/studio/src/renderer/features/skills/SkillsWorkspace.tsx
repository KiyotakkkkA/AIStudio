import {
  mdiBookOpenOutline,
  mdiContentCopy,
  mdiDeleteOutline,
  mdiDotsHorizontal,
  mdiExport,
  mdiFolderDownloadOutline,
  mdiPlus,
  mdiUpdate,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { useEffect, useState } from "react";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "../../ui/molecules/ActionMenu";
import EmptyState from "../../ui/molecules/EmptyState";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import SkillFileTree from "./SkillFileTree";
import SkillFileView from "./SkillFileView";
import SkillList from "./SkillList";
import SkillManifestView from "./SkillManifestView";
import SkillRail from "./SkillRail";
import { MANIFEST_PATH } from "./SkillStore";
import {
  headerLine,
  UPDATE_ALL_TITLE,
  UPDATE_CONFIRM_LABEL,
  UPDATE_MODIFIED_WARNING,
  UPDATE_SKILL_TITLE,
  updateAllLabel,
  updateAllWarning,
  updateLabel,
} from "./skillPresentation";
import Alert from "../../ui/molecules/Alert";

function SkillsWorkspace() {
  const { skills: store } = useStore();
  const form = store.form;
  const selected = store.selectedSkill;
  const update = selected?.update ?? null;

  const [confirmRemove, setConfirmRemove] = useState(false);

  useEffect(() => {
    void store.mount();
    return store.dispose;
  }, [store]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiBookOpenOutline}
        title="Навыки"
        subtitle={headerLine(store.skills)}
        actions={
          <>
            {store.updatable.length === 0 ? null : (
              <IconButton
                path={mdiUpdate}
                label={updateAllLabel(store.updatable.length)}
                tooltip="bottom-center"
                iconSize={16}
                size="md"
                className="bg-main-800"
                disabled={store.updatingAll}
                needConfirm={store.modifiedUpdates > 0}
                modalSetup={{
                  title: UPDATE_ALL_TITLE,
                  content: updateAllWarning(store.modifiedUpdates),
                  confirmLabel: UPDATE_CONFIRM_LABEL,
                }}
                onClick={() => void store.updateAll()}
              />
            )}
            <Button
              tone="secondary"
              disabled={store.importing}
              onClick={() => void store.importFolder()}
            >
              <Icon path={mdiFolderDownloadOutline} size={15} />
              Импорт папки
            </Button>
            <Button tone="primary" onClick={store.requestCreate}>
              <Icon path={mdiPlus} size={15} />
              Новый навык
            </Button>
          </>
        }
      />

      {store.error === null ? null : (
        <Alert className="mx-6 mt-3" onDismiss={store.dismissError}>
          {store.error}
        </Alert>
      )}
      {store.notice === null ? null : (
        <div className="mx-6 mt-3 flex flex-none items-center gap-3 rounded-control bg-ok-soft px-3 py-2.25 text-[12px] text-ok">
          <span className="flex-1">{store.notice}</span>
          <button type="button" className="text-main-400" onClick={store.dismissNotice}>
            Скрыть
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <SkillList store={store} />

        {form === null ? (
          <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
            <EmptyState
              icon={mdiBookOpenOutline}
              title={store.isEmpty ? "Навыков нет" : "Навык не выбран"}
              description={
                store.isEmpty
                  ? "Навык — инструкция с описанием, по которому она срабатывает, и файлами, которые ей нужны."
                  : "Выберите навык слева или создайте новый."
              }
              action={
                <Button tone="primary" onClick={store.requestCreate}>
                  <Icon path={mdiPlus} size={15} />
                  Новый навык
                </Button>
              }
            />
          </div>
        ) : (
          <>
            <div className="flex min-w-0 flex-1 flex-col">
              <div className="flex h-11.5 flex-none items-center gap-1.5 border-b border-line px-4.5">
                <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-main-50">
                  {form.isNew ? "Новый навык" : form.name}
                </span>
                {selected === null || update === null || form.isNew ? null : (
                  <IconButton
                    key={`${selected.id}:${update.version}`}
                    path={mdiUpdate}
                    label={updateLabel(update)}
                    tooltip="bottom-center"
                    iconSize={17}
                    size="md"
                    disabled={store.isUpdating(selected.id)}
                    needConfirm={update.modified}
                    modalSetup={{
                      title: UPDATE_SKILL_TITLE,
                      content: UPDATE_MODIFIED_WARNING,
                      confirmLabel: UPDATE_CONFIRM_LABEL,
                    }}
                    onClick={() => void store.updateSkill(selected.id)}
                  />
                )}
                {store.selectedSkill === null || form.isNew ? null : (
                  <Toggle
                    label="Включён"
                    checked={store.selectedSkill.enabled}
                    disabled={store.isSwitching(store.selectedSkill.id)}
                    onChange={(checked) => {
                      const skill = store.selectedSkill;
                      if (skill !== null) void store.setEnabled(skill.id, checked);
                    }}
                  />
                )}
                {form.isNew ? null : (
                  <Dropdown menuWidth={220} menuPlacement="bottom-right">
                    <IconDropdownButton
                      label="Действия с навыком"
                      tooltip="bottom-center"
                      path={mdiDotsHorizontal}
                    />
                    <ActionMenu label="Действия с навыком">
                      <ActionMenuItem icon={mdiContentCopy} onClick={store.requestDuplicate}>
                        Дублировать
                      </ActionMenuItem>
                      <ActionMenuItem icon={mdiExport} onClick={() => void store.exportSelected()}>
                        Экспорт в папку
                      </ActionMenuItem>
                      <ActionMenuSeparator />
                      <ActionMenuItem
                        icon={mdiDeleteOutline}
                        danger
                        disabled={store.removing}
                        onClick={() => setConfirmRemove(true)}
                      >
                        Удалить навык
                      </ActionMenuItem>
                    </ActionMenu>
                  </Dropdown>
                )}
              </div>
              <div className="flex min-h-0 flex-1">
                <SkillFileTree store={store} vm={form} />
                {store.openPath === MANIFEST_PATH ? (
                  <SkillManifestView store={store} vm={form} />
                ) : (
                  <SkillFileView store={store} vm={form} />
                )}
              </div>
              <div className="flex flex-none items-center justify-end gap-3 border-t border-line bg-main-900 px-4.5 py-3">
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
                  onClick={() => void store.submit()}
                >
                  {store.saving ? "Сохранение…" : "Сохранить"}
                </Button>
              </div>
            </div>
            <SkillRail vm={form} />
          </>
        )}
      </div>

      <ConfirmDialog
        open={confirmRemove && form !== null && !form.isNew}
        title="Удалить навык?"
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirmRemove(false)}
        onConfirm={() => {
          setConfirmRemove(false);
          void store.removeSelected();
        }}
      >
        Папка навыка «{form?.name}» со всеми файлами будет удалена. Действие нельзя отменить.
      </ConfirmDialog>
      <ConfirmDialog
        open={store.pending !== null}
        title="Есть несохранённые изменения"
        confirmLabel="Не сохранять"
        confirmTone="danger"
        cancelLabel="Остаться"
        onConfirm={store.confirmPending}
        onCancel={store.cancelPending}
      >
        Изменения в навыке не сохранены. Если продолжить, они будут потеряны.
      </ConfirmDialog>
    </div>
  );
}

export default observer(SkillsWorkspace);
