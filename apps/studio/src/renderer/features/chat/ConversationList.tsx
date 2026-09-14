import { observer } from "mobx-react-lite";
import { mdiDotsVertical } from "@mdi/js";
import { Dropdown, Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useState } from "react";
import Button from "../../ui/atoms/Button";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import type { ConversationDto } from "@zvs/shared";
import type ChatStore from "./ChatStore";

export default observer(function ConversationList({ store }: { readonly store: ChatStore }) {
  const [renameTarget, setRenameTarget] = useState<ConversationDto | null>(null);
  const [renameTitle, setRenameTitle] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<ConversationDto | null>(null);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const week = new Date(today);
  week.setDate(week.getDate() - ((week.getDay() + 6) % 7));
  const groups = [
    {
      name: "Сегодня",
      rows: store.visibleConversations.filter((item) => item.updatedAt >= +today),
    },
    {
      name: "Ранее на этой неделе",
      rows: store.visibleConversations.filter(
        (item) => item.updatedAt < +today && item.updatedAt >= +week,
      ),
    },
    { name: "Ранее", rows: store.visibleConversations.filter((item) => item.updatedAt < +week) },
  ];
  return (
    <aside
      aria-label="Диалоги"
      className="flex w-62.5 shrink-0 flex-col border-r border-main-750 max-[850px]:w-45"
    >
      <div className="flex h-header shrink-0 items-center px-3">
        <Button onClick={store.newChat} disabled={store.generating}>
          Новый чат
        </Button>
      </div>
      <div className="px-3 pb-3">
        <TextInput
          preset="search"
          aria-label=""
          placeholder="Поиск диалогов"
          value={store.query}
          onChange={(event) => store.setQuery(event.target.value)}
        />
      </div>
      <ScrollArea className="min-h-0 flex-1 px-2 pb-3">
        {groups.map(
          (group) =>
            group.rows.length > 0 && (
              <section key={group.name} className="mb-4">
                <h2 className="px-2 pb-2 text-[10px] font-medium tracking-wider text-main-500 uppercase">
                  {group.name}
                </h2>
                {group.rows.map((conversation) => (
                  <div
                    onClick={() => void store.select(conversation.id)}
                    key={conversation.id}
                    className={`group mb-1 flex w-full items-center rounded-lg p-1.5 ${store.active?.id === conversation.id ? "bg-main-700" : "hover:bg-main-900"}`}
                  >
                    <div className="flex min-w-0 flex-1 items-center gap-2 text-left disabled:opacity-60">
                      <span
                        className={`size-1.5 shrink-0 rounded-full ${store.active?.id === conversation.id ? "bg-accent-dark" : "bg-main-600"}`}
                      />
                      <span className="truncate text-[12.5px]">{conversation.title}</span>
                    </div>
                    <Dropdown menuWidth={160} menuPlacement="bottom-right">
                      <Dropdown.Trigger
                        rounded="rounded-md"
                        aria-label={`Действия для ${conversation.title}`}
                        title="Действия"
                        className="invisible group-hover:visible group-focus-within:visible gap-0 p-0 size-6 justify-center hover:border-transparent border-transparent bg-transparent hover:bg-main-750"
                        icon={<Icon path={mdiDotsVertical} size={20} />}
                        onClick={(event) => event.stopPropagation()}
                      >
                        <></>
                      </Dropdown.Trigger>
                      <Dropdown.Menu
                        role="menu"
                        aria-label="Действия диалога"
                        rounded="rounded-md"
                        className="rounded-card border-main-750"
                      >
                        <Dropdown.Item
                          role="menuitem"
                          rounded="rounded-md"
                          className="text-[12.5px] text-main-100"
                          onClick={() => {
                            setRenameTarget(conversation);
                            setRenameTitle(conversation.title);
                          }}
                        >
                          Переименовать
                        </Dropdown.Item>
                        <Dropdown.Item
                          role="menuitem"
                          rounded="rounded-md"
                          className="text-[12.5px] text-red-300"
                          onClick={() => setDeleteTarget(conversation)}
                        >
                          Удалить
                        </Dropdown.Item>
                      </Dropdown.Menu>
                    </Dropdown>
                  </div>
                ))}
              </section>
            ),
        )}
        {!store.loading && store.visibleConversations.length === 0 && (
          <p className="p-2 text-xs text-main-400">
            {store.query ? "Подходящих диалогов нет." : "Здесь появятся ваши диалоги."}
          </p>
        )}
      </ScrollArea>
      <Modal
        open={renameTarget !== null}
        onClose={() => setRenameTarget(null)}
        label="Переименовать диалог"
        className="w-105 max-w-[92vw] rounded-card border border-main-750 bg-main-900 p-4.5"
      >
        <h2 className="text-[14px] font-semibold text-main-50">Переименовать диалог</h2>
        <div className="mt-3">
          <TextInput
            aria-label="Название диалога"
            value={renameTitle}
            autoFocus
            onChange={(event) => setRenameTitle(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || !renameTarget) return;
              event.preventDefault();
              void store
                .saveConversationTitle(renameTarget.id, renameTitle)
                .then(() => setRenameTarget(null));
            }}
          />
        </div>
        <div className="mt-4.5 flex justify-end gap-2">
          <Button type="button" tone="secondary" onClick={() => setRenameTarget(null)}>
            Отмена
          </Button>
          <Button
            type="button"
            tone="primary"
            disabled={!renameTitle.trim() || store.generating}
            onClick={() => {
              if (!renameTarget) return;
              void store
                .saveConversationTitle(renameTarget.id, renameTitle)
                .then(() => setRenameTarget(null));
            }}
          >
            Сохранить
          </Button>
        </div>
      </Modal>
      <ConfirmModal
        open={deleteTarget !== null}
        title="Удалить диалог?"
        content={
          deleteTarget === null
            ? null
            : `Диалог «${deleteTarget.title}» будет удалён без возможности восстановления.`
        }
        confirmLabel="Удалить"
        cancelLabel="Отмена"
        tone="danger"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => {
          if (!deleteTarget) return;
          void store.removeConversation(deleteTarget.id).then(() => setDeleteTarget(null));
        }}
      />
    </aside>
  );
});
