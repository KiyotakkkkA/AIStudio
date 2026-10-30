import { observer } from "mobx-react-lite";
import {
  mdiChatOutline,
  mdiCodeJson,
  mdiDeleteOutline,
  mdiDockLeft,
  mdiDotsVertical,
  mdiExportVariant,
  mdiLanguageMarkdownOutline,
  mdiPencilOutline,
  mdiPinOffOutline,
  mdiPinOutline,
  mdiSquareEditOutline,
} from "@mdi/js";
import { ContextMenu, Dropdown, Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { Fragment, useState, type MouseEvent } from "react";
import { AnimatePresence, motion } from "motion/react";
import ActivePill from "../../ui/motion/ActivePill";
import { useFx, useTransition } from "../../ui/motion/fx";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "../../ui/molecules/ActionMenu";
import TextInput from "../../ui/atoms/TextInput";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import type { ConversationDto } from "@zvs/shared";
import type ChatStore from "./ChatStore";
import { HIDE_CONVERSATIONS } from "./conversationListCopy";
import { PINNED_GROUP, PIN_CONVERSATION, UNPIN_CONVERSATION } from "./pinnedMessages";
import { EXPORT, EXPORT_FORMATS } from "./ConversationExport";

interface ConversationAction {
  readonly key: string;
  readonly icon: string;
  readonly label: string;
  readonly danger?: boolean;
  readonly run: () => void;
}

const CONTEXT_MENU = "w-48 rounded-card border-line bg-main-900 p-1 shadow-pop backdrop-blur-none";

function select(action: ConversationAction) {
  return (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    action.run();
  };
}

export default observer(function ConversationList({ store }: { readonly store: ChatStore }) {
  const [renameTarget, setRenameTarget] = useState<ConversationDto | null>(null);
  const [renameTitle, setRenameTitle] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<ConversationDto | null>(null);
  const { motion: level } = useFx();
  const settle = useTransition("layout");
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const week = new Date(today);
  week.setDate(week.getDate() - ((week.getDay() + 6) % 7));
  const unpinned = store.visibleConversations.filter((item) => !item.pinned);
  const groups = [
    { name: PINNED_GROUP, rows: store.visibleConversations.filter((item) => item.pinned) },
    { name: "Сегодня", rows: unpinned.filter((item) => item.updatedAt >= +today) },
    {
      name: "Ранее на этой неделе",
      rows: unpinned.filter((item) => item.updatedAt < +today && item.updatedAt >= +week),
    },
    { name: "Ранее", rows: unpinned.filter((item) => item.updatedAt < +week) },
  ];
  const actions = (conversation: ConversationDto): ConversationAction[] => [
    {
      key: "rename",
      icon: mdiPencilOutline,
      label: "Переименовать",
      run: () => {
        setRenameTarget(conversation);
        setRenameTitle(conversation.title);
      },
    },
    {
      key: "pin",
      icon: conversation.pinned ? mdiPinOffOutline : mdiPinOutline,
      label: conversation.pinned ? UNPIN_CONVERSATION : PIN_CONVERSATION,
      run: () => void store.pinConversation(conversation.id, !conversation.pinned),
    },
    {
      key: "remove",
      icon: mdiDeleteOutline,
      label: "Удалить",
      danger: true,
      run: () => setDeleteTarget(conversation),
    },
  ];
  if (!store.listOpen) return null;
  return (
    <aside
      aria-label="Диалоги"
      className="flex w-66 shrink-0 flex-col bg-main-800 max-[850px]:w-50"
    >
      <div className="flex h-14 shrink-0 items-center gap-2.5 pr-2.5 pl-4">
        <span className="flex size-7 flex-none items-center justify-center rounded-control bg-accent-soft text-accent-medium">
          <Icon path={mdiChatOutline} size={15} />
        </span>
        <h1 className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-[-0.01em] text-main-50">
          Чат
        </h1>
        <IconButton
          label="Новый чат"
          tooltip="bottom-center"
          path={mdiSquareEditOutline}
          iconSize={17}
          disabled={store.generating}
          onClick={store.newChat}
        />
        <IconButton
          label={HIDE_CONVERSATIONS}
          tooltip="bottom-center"
          path={mdiDockLeft}
          iconSize={17}
          onClick={store.toggleList}
        />
      </div>
      <div className="shrink-0 px-3 pb-2">
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
                <AnimatePresence initial={false}>
                  {group.rows.map((conversation) => (
                    <motion.div
                      onClick={() => void store.select(conversation.id)}
                      key={conversation.id}
                      layout={level === "full" ? "position" : false}
                      initial={{ opacity: 0, height: 0 }}
                      animate={{ opacity: 1, height: "auto" }}
                      exit={{ opacity: 0, height: 0 }}
                      transition={settle}
                      className="overflow-hidden"
                    >
                      <ContextMenu>
                        <ContextMenu.Trigger
                          className={`group relative mb-0.5 flex h-8.5 w-full cursor-pointer items-center rounded-control pr-1 pl-2.5 ${
                            store.active?.id === conversation.id
                              ? "text-main-50"
                              : "text-main-300 hover:bg-main-700 hover:text-main-100"
                          }`}
                        >
                          {store.active?.id === conversation.id ? (
                            <ActivePill
                              layoutId="conversation-active"
                              className="rounded-control bg-main-900 fx-on:shadow-[0_1px_3px_rgb(40_25_10/0.08)]"
                            />
                          ) : null}
                          <div className="relative flex min-w-0 flex-1 items-center gap-2.5 text-left">
                            <span
                              className={`size-1.5 shrink-0 rounded-full ${store.active?.id === conversation.id ? "bg-accent-dark" : "bg-main-600"}`}
                            />
                            <span className="truncate text-[13px]">{conversation.title}</span>
                          </div>
                          <Dropdown menuWidth={160} menuPlacement="bottom-right">
                            <IconDropdownButton
                              aria-label={`Действия для ${conversation.title}`}
                              label="Действия"
                              tooltip="right-center"
                              path={mdiDotsVertical}
                              size="xs"
                              className="invisible group-focus-within:visible group-hover:visible"
                              onClick={(event) => event.stopPropagation()}
                            />
                            <ActionMenu label="Действия диалога">
                              {actions(conversation).map((action) => (
                                <Fragment key={action.key}>
                                  {action.danger && <ActionMenuSeparator />}
                                  <ActionMenuItem
                                    icon={action.icon}
                                    danger={action.danger ?? false}
                                    onClick={select(action)}
                                  >
                                    {action.label}
                                  </ActionMenuItem>
                                </Fragment>
                              ))}
                            </ActionMenu>
                          </Dropdown>
                        </ContextMenu.Trigger>
                        <ContextMenu.Content aria-label="Действия диалога" className={CONTEXT_MENU}>
                          {actions(conversation).map((action) => (
                            <Fragment key={action.key}>
                              {action.danger && (
                                <>
                                  <ContextMenu.Sub>
                                    <ContextMenu.SubTrigger
                                      className="text-[13px] text-main-100"
                                      leftSlot={<Icon path={mdiExportVariant} size={16} />}
                                      disabled={store.exports.busy}
                                    >
                                      {EXPORT}
                                    </ContextMenu.SubTrigger>
                                    <ContextMenu.SubContent
                                      aria-label={EXPORT}
                                      className={CONTEXT_MENU}
                                    >
                                      {EXPORT_FORMATS.map((option) => (
                                        <ContextMenu.Item
                                          key={option.format}
                                          className="text-[13px] text-main-100"
                                          leftSlot={
                                            <Icon
                                              path={
                                                option.format === "json"
                                                  ? mdiCodeJson
                                                  : mdiLanguageMarkdownOutline
                                              }
                                              size={16}
                                            />
                                          }
                                          onClick={(event) => {
                                            event.stopPropagation();
                                            void store.exports.export(
                                              conversation.id,
                                              option.format,
                                            );
                                          }}
                                        >
                                          {option.label}
                                        </ContextMenu.Item>
                                      ))}
                                    </ContextMenu.SubContent>
                                  </ContextMenu.Sub>
                                  <ContextMenu.Separator className="mx-2 my-1 h-px bg-line" />
                                </>
                              )}
                              {action.danger ? (
                                <ContextMenu.ItemDanger
                                  className="text-[13px] text-err hover:bg-err-soft hover:text-err"
                                  leftSlot={<Icon path={action.icon} size={16} />}
                                  onClick={select(action)}
                                >
                                  {action.label}
                                </ContextMenu.ItemDanger>
                              ) : (
                                <ContextMenu.Item
                                  className="text-[13px] text-main-100"
                                  leftSlot={<Icon path={action.icon} size={16} />}
                                  onClick={select(action)}
                                >
                                  {action.label}
                                </ContextMenu.Item>
                              )}
                            </Fragment>
                          ))}
                        </ContextMenu.Content>
                      </ContextMenu>
                    </motion.div>
                  ))}
                </AnimatePresence>
              </section>
            ),
        )}
        {!store.loading && store.visibleConversations.length === 0 && (
          <p className="px-2.5 py-3 text-[12px] text-main-500">
            {store.query ? "Подходящих диалогов нет." : "Диалогов пока нет."}
          </p>
        )}
      </ScrollArea>
      <Modal
        open={renameTarget !== null}
        onClose={() => setRenameTarget(null)}
        label="Переименовать диалог"
        className="w-110 max-w-[92vw] overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
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
        <div className="mt-5 flex justify-end gap-2 bg-main-850 px-5 py-3">
          <Button tone="secondary" onClick={() => setRenameTarget(null)}>
            Отмена
          </Button>
          <Button
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
