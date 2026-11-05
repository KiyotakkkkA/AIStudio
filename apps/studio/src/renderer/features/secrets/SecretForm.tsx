import { mdiContentCopy, mdiDeleteOutline, mdiDotsHorizontal, mdiLockOutline } from "@mdi/js";
import { useState } from "react";
import { observer } from "mobx-react-lite";
import { Dropdown, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import TagsInput from "../../ui/molecules/TagsInput";
import { ROTATION_OPTIONS, type SecretFormVm } from "./SecretFormVm";
import { maskedHint, SCOPE_OPTIONS } from "./secretPresentation";
import Alert from "../../ui/molecules/Alert";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "../../ui/molecules/ActionMenu";

interface SecretFormProps {
  readonly vm: SecretFormVm;
  readonly hint: string | null;
  readonly saving: boolean;
  readonly onSave: () => void;
  readonly onCancel: () => void;
  readonly onDuplicate: () => void;
  readonly onDelete: () => void;
}

function SecretForm({
  vm,
  hint,
  saving,
  onSave,
  onCancel,
  onDuplicate,
  onDelete,
}: SecretFormProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);

  return (
    <form
      className="flex min-h-0 min-w-0 flex-1 flex-col"
      onSubmit={(event) => {
        event.preventDefault();
        onSave();
      }}
    >
      <div className="mx-auto flex w-full max-w-360 flex-none items-center gap-2.5 px-5 pt-4 pb-2 lg:px-6">
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-[14px] font-semibold text-main-50">
            {vm.name.trim().length > 0 ? vm.name : "Создать секрет"}
          </h2>
        </div>
        {vm.isNew ? null : (
          <Dropdown menuWidth={220} menuPlacement="bottom-right" disabled={saving}>
            <IconDropdownButton
              label="Действия с секретом"
              tooltip="bottom-right"
              path={mdiDotsHorizontal}
              disabled={saving}
            />
            <ActionMenu label="Действия с секретом">
              <ActionMenuItem icon={mdiContentCopy} onClick={onDuplicate}>
                Дублировать
              </ActionMenuItem>
              <ActionMenuSeparator />
              <ActionMenuItem icon={mdiDeleteOutline} danger onClick={() => setConfirmDelete(true)}>
                Удалить секрет
              </ActionMenuItem>
            </ActionMenu>
          </Dropdown>
        )}
      </div>

      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-4 px-5 py-3 lg:px-6">
        {vm.banner === null ? null : <Alert>{vm.banner}</Alert>}

        <section className="mx-auto flex w-full max-w-360 flex-col gap-4 rounded-xl bg-main-850 p-5">
          <h3 className="text-[15px] font-semibold tracking-[-0.01em] text-main-100">Общее</h3>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field
              label="Отображаемое имя"
              htmlFor="secret-name"
              required
              error={vm.errorOf("name")}
            >
              <TextInput
                id="secret-name"
                value={vm.name}
                maxLength={128}
                placeholder="Ollama — личный"
                invalid={vm.errorOf("name") !== undefined}
                onChange={(event) => {
                  vm.setName(event.target.value);
                }}
              />
            </Field>
            <Field
              label="Значение"
              htmlFor="secret-value"
              required={vm.valueRequired}
              optionalHint={!vm.valueRequired}
              help={vm.isNew ? undefined : "Пустое поле сохраняет текущее значение секрета."}
              error={vm.errorOf("value")}
            >
              <TextInput
                id="secret-value"
                preset="password"
                autoComplete="off"
                spellCheck={false}
                mono
                value={vm.value}
                invalid={vm.errorOf("value") !== undefined}
                placeholder={vm.isNew ? "" : maskedHint(hint)}
                onChange={(event) => {
                  vm.setValue(event.target.value);
                }}
              />
            </Field>
          </div>
          <Field label="Область видимости">
            <SegmentedControl
              label="Область видимости"
              value={vm.scope}
              options={SCOPE_OPTIONS}
              onChange={vm.setScope}
            />
          </Field>
        </section>

        <section className="mx-auto flex w-full max-w-360 flex-col gap-4 rounded-xl bg-main-850 p-5">
          <h3 className="text-[15px] font-semibold tracking-[-0.01em] text-main-100">Метаданные</h3>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field label="Теги" htmlFor="secret-tags">
              <TagsInput id="secret-tags" tags={vm.tags} onChange={vm.setTags} />
            </Field>
            <Field label="Напоминание о ротации">
              <SelectInput
                label="Напоминание о ротации"
                value={vm.rotationDays === null ? "" : String(vm.rotationDays)}
                options={[...ROTATION_OPTIONS]}
                onChange={(value) => {
                  vm.setRotationDays(value === "" ? null : Number(value));
                }}
              />
            </Field>
          </div>
          <Field label="Заметка" htmlFor="secret-note">
            <TextArea
              id="secret-note"
              value={vm.note}
              maxLength={2000}
              placeholder="Назначение ключа и ограничения на его использование"
              onChange={(event) => {
                vm.setNote(event.target.value);
              }}
            />
          </Field>
        </section>
      </ScrollArea>

      <div className="flex-none px-5 pt-2 pb-4 lg:px-6">
        <div className="mx-auto flex w-full max-w-360 flex-wrap items-center gap-3 rounded-xl bg-main-800 px-4 py-3 shadow-[0_6px_24px_rgb(0_0_0/0.12)]">
          <Icon path={mdiLockOutline} size={15} className="flex-none text-ok" />
          <span
            className="line-clamp-2 min-w-0 flex-1 text-[12px] text-main-400"
            title="Значение шифруется хранилищем учётных данных операционной системы и не покидает это устройство."
          >
            Зашифрованное хранение
          </span>
          <Button
            tone="ghost"
            onClick={onCancel}
            disabled={saving}
            needConfirm
            modalSetup={{
              title: "Отменить?",
              content: "Несохранённые изменения будут потеряны.",
              confirmLabel: "Отменить",
            }}
          >
            Отмена
          </Button>
          <Button type="submit" tone="primary" disabled={saving}>
            {saving ? "Сохранение…" : "Сохранить секрет"}
          </Button>
        </div>
      </div>
      <ConfirmDialog
        open={confirmDelete && !vm.isNew}
        title="Удалить секрет?"
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          onDelete();
        }}
      >
        Секрет «{vm.name}» будет удалён. Это действие нельзя отменить.
      </ConfirmDialog>
    </form>
  );
}

export default observer(SecretForm);
