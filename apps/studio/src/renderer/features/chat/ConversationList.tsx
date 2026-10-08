import { observer } from "mobx-react-lite";
import { mdiDeleteOutline, mdiDotsVertical, mdiPencilOutline, mdiPlus } from "@mdi/js";
import { Dropdown, Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useState } from "react";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
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
      className="flex w-66 shrink-0 flex-col border-r border-main-750 bg-main-850 max-[850px]:w-50"
    >
      <div className="flex shrink-0 flex-col gap-2.5 p-3">
        <Button tone="primary" onClick={store.newChat} disabled={store.generating}>
          <Icon path={mdiPlus} size={16} />
          Новый чат
        </Button>
        <TextInput
          preset="search"
          aria-label=""
          placeholder="Поиск диалогов"
          value={store.query}
          onChange={(event) => store.setQuery(event.target.value)}
        />
      </div>
      <ScrollArea className="min-h-0 flex-1 px-2 pt-1 pb-3">
        {groups.map(
          (group) =>
            group.rows.length > 0 && (
              <section key={group.name} className="mb-3">
                <h2 className="px-2.5 pt-2 pb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                  {group.name}
                </h2>
                {group.rows.map((conversation) => (
                  <div
                    onClick={() => void store.select(conversation.id)}
                    key={conversation.id}
                    className={`group mb-0.5 flex h-8.5 w-full cursor-pointer items-center rounded-control pr-1 pl-2.5 ${
                      store.active?.id === conversation.id
                        ? "bg-main-900 text-main-50 shadow-[0_1px_2px_rgb(40_25_10/0.08)] ring-1 ring-main-750 ring-inset"
                        : "text-main-300 hover:bg-main-700 hover:text-main-100"
                    }`}
                  >
                    <div className="flex min-w-0 flex-1 items-center gap-2.5 text-left">
                      <span
                        className={`size-1.5 shrink-0 rounded-full ${store.active?.id === conversation.id ? "bg-accent-dark" : "bg-main-600"}`}
                      />
                      <span className="truncate text-[13px]">{conversation.title}</span>
                    </div>
                    <Dropdown menuWidth={160} menuPlacement="bottom-right">
                      <IconDropdownButton
                        rounded="rounded-lg"
                        aria-label={`Действия для ${conversation.title}`}
                        label="Действия"
                        path={mdiDotsVertical}
                        className="invisible size-6 justify-center gap-0 border-transparent bg-transparent p-0 text-main-400 group-focus-within:visible group-hover:visible hover:border-transparent hover:bg-main-700 hover:text-main-100"
                        iconSize={18}
                        onClick={(event) => event.stopPropagation()}
                      />
                      <Dropdown.Menu
                        role="menu"
                        aria-label="Действия диалога"
                        rounded="rounded-lg"
                        className="rounded-card border-main-750 bg-main-900 p-1 shadow-pop"
                      >
                        <Dropdown.Item
                          role="menuitem"
                          rounded="rounded-lg"
                          className="text-[13px] text-main-100"
                          onClick={(event) => {
                            event.stopPropagation();
                            setRenameTarget(conversation);
                            setRenameTitle(conversation.title);
                          }}
                        >
                          <span className="flex items-center gap-2">
                            <Icon path={mdiPencilOutline} size={16} className="text-main-400" />
                            Переименовать
                          </span>
                        </Dropdown.Item>
                        <Dropdown.Item
                          role="menuitem"
                          rounded="rounded-lg"
                          className="text-[13px] text-err hover:bg-err-soft hover:text-err"
                          onClick={(event) => {
                            event.stopPropagation();
                            setDeleteTarget(conversation);
                          }}
                        >
                          <span className="flex items-center gap-2">
                            <Icon path={mdiDeleteOutline} size={16} />
                            Удалить
                          </span>
                        </Dropdown.Item>
                      </Dropdown.Menu>
                    </Dropdown>
                  </div>
                ))}
              </section>
            ),
        )}
        {!store.loading && store.visibleConversations.length === 0 && (
          <p className="px-2.5 py-3 text-[12px] text-main-500">
            {store.query ? "Подходящих диалогов нет." : "Здесь появятся ваши диалоги."}
          </p>
        )}
      </ScrollArea>
      <Modal
        open={renameTarget !== null}
        onClose={() => setRenameTarget(null)}
        label="Переименовать диалог"
        className="w-110 max-w-[92vw] overflow-hidden rounded-card border border-main-750 bg-main-900 p-0 shadow-pop"
      >
        <div className="px-5 pt-5">
          <h2 className="text-[15px] font-semibold tracking-[-0.01em] text-main-50">
            Переименовать диалог
          </h2>
        </div>
        <div className="mt-3 px-5">
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
        <div className="mt-5 flex justify-end gap-2 border-t border-main-750 bg-main-850 px-5 py-3">
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
