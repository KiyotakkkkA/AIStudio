import {
  mdiBookOpenOutline,
  mdiContentCopy,
  mdiDeleteOutline,
  mdiDotsHorizontal,
  mdiExport,
  mdiFolderDownloadOutline,
  mdiPlus,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { useEffect } from "react";
import Button from "../../ui/atoms/buttons/Button";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import EmptyState from "../../ui/molecules/EmptyState";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import SkillFileTree from "./SkillFileTree";
import SkillFileView from "./SkillFileView";
import SkillList from "./SkillList";
import SkillManifestView from "./SkillManifestView";
import SkillRail from "./SkillRail";
import { MANIFEST_PATH } from "./SkillStore";
import { headerLine } from "./skillPresentation";

const MENU_ITEM = "text-[13px] text-main-100";

function SkillsWorkspace() {
  const { skills: store } = useStore();
  const form = store.form;

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
                      tooltip="bottom-left"
                      path={mdiDotsHorizontal}
                      className="size-8 justify-center gap-0 border-transparent bg-transparent p-0 text-main-400 hover:bg-main-700 hover:text-main-100"
                      rounded="rounded-lg"
                      iconSize={18}
                    />
                    <Dropdown.Menu
                      role="menu"
                      aria-label="Действия с навыком"
                      rounded="rounded-lg"
                      className="rounded-card border-line bg-main-900 p-1 shadow-pop"
                    >
                      <Dropdown.Item
                        role="menuitem"
                        rounded="rounded-lg"
                        className={MENU_ITEM}
                        onClick={store.requestDuplicate}
                      >
                        <span className="flex items-center gap-2">
                          <Icon path={mdiContentCopy} size={16} />
                          Дублировать
                        </span>
                      </Dropdown.Item>
                      <Dropdown.Item
                        role="menuitem"
                        rounded="rounded-lg"
                        className={MENU_ITEM}
                        onClick={() => void store.exportSelected()}
                      >
                        <span className="flex items-center gap-2">
                          <Icon path={mdiExport} size={16} />
                          Экспорт в папку
                        </span>
                      </Dropdown.Item>
                    </Dropdown.Menu>
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
              <div className="flex flex-none items-center gap-3 border-t border-line bg-main-900 px-4.5 py-3">
                {form.isNew ? null : (
                  <Button
                    key={form.skillId}
                    tone="danger"
                    disabled={store.removing}
                    needConfirm
                    modalSetup={{
                      title: "Удалить навык?",
                      content: (
                        <>
                          Папка навыка «{form.name}» со всеми файлами будет удалена. Действие нельзя
                          отменить.
                        </>
                      ),
                      tone: "danger",
                      confirmLabel: "Удалить",
                    }}
                    onClick={() => void store.removeSelected()}
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
                  onClick={() => void store.submit()}
                >
                  {store.saving ? "Сохранение…" : "Сохранить"}
                </Button>
              </div>
            </div>
            <SkillRail store={store} vm={form} />
          </>
        )}
      </div>

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
