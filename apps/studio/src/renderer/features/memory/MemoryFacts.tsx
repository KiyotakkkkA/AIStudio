import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import {
  mdiAccountOutline,
  mdiBrain,
  mdiCreationOutline,
  mdiDeleteOutline,
  mdiForumOutline,
  mdiPencilOutline,
} from "@mdi/js";
import { MEMORY_CONTENT_MAX, type ConversationId, type MemoryDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import EmptyState from "../../ui/molecules/EmptyState";
import TextComposer from "../../ui/molecules/TextComposer";
import { useStore } from "../../stores/useStore";
import {
  ACCOUNT_EMPTY,
  ACCOUNT_EMPTY_TITLE,
  ACCOUNT_HINT,
  ACCOUNT_PLACEHOLDER,
  ACCOUNT_TAB,
  ADD,
  CANCEL,
  CLEAR,
  CLEAR_ACCOUNT_TITLE,
  CLEAR_CONSEQUENCE,
  CLEAR_DIALOG_TITLE,
  DIALOG_EMPTY,
  DIALOG_EMPTY_TITLE,
  DIALOG_HINT,
  DIALOG_PICKER,
  DIALOG_PLACEHOLDER,
  DIALOG_TAB,
  EDIT,
  LOADING,
  NO_DIALOGS,
  REMOVE,
  REMOVE_TITLE,
  SAVE,
  SCOPE_TABS_LABEL,
  SOURCE_MODEL,
  SOURCE_USER,
  factsLabel,
  memoryDate,
} from "./memoryCopy";
import Alert from "../../ui/molecules/Alert";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { LoadingStatus } from "../../ui/molecules/LoadingSkeleton";

type Scope = "account" | "dialog";

const MemoryRow = observer(function MemoryRow({ memory }: { readonly memory: MemoryDto }) {
  const { memory: store } = useStore();
  const [editing, setEditing] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const byModel = memory.source === "model";
  if (editing !== null)
    return (
      <li className="px-3 py-3">
        <TextComposer
          label={EDIT}
          placeholder={ACCOUNT_PLACEHOLDER}
          value={editing}
          max={MEMORY_CONTENT_MAX}
          autoFocus
          submitLabel={SAVE}
          canSubmit={editing.trim().length > 0 && editing.trim() !== memory.content}
          disabled={store.saving}
          cancelLabel={CANCEL}
          onCancel={() => setEditing(null)}
          onChange={setEditing}
          onSubmit={() => {
            void store.edit(memory.id, editing).then((saved) => {
              if (saved) setEditing(null);
            });
          }}
        />
      </li>
    );
  return (
    <li className="group flex items-start gap-3 px-4 py-3 transition-colors hover:bg-main-850">
      <span
        className={`mt-0.5 flex size-7 flex-none items-center justify-center rounded-control ${
          byModel ? "bg-accent-soft text-accent-medium" : "bg-main-700 text-main-300"
        }`}
      >
        <Icon path={byModel ? mdiCreationOutline : mdiAccountOutline} size={15} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-[13px] leading-normal wrap-break-word text-main-100">
          {memory.content}
        </span>
        <span className="flex items-center gap-2">
          <Chip tone={byModel ? "accent" : "neutral"}>{byModel ? SOURCE_MODEL : SOURCE_USER}</Chip>
          <span className="text-[11.5px] text-main-500">{memoryDate(memory)}</span>
        </span>
      </div>
      <div className="flex flex-none items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
        <IconButton
          path={mdiPencilOutline}
          label={EDIT}
          disabled={store.saving}
          onClick={() => setEditing(memory.content)}
        />
        <IconButton
          path={mdiDeleteOutline}
          label={REMOVE}
          disabled={store.saving}
          onClick={() => setRemoving(true)}
        />
      </div>
      <ConfirmModal
        open={removing}
        title={REMOVE_TITLE}
        content={memory.content}
        confirmLabel={REMOVE}
        disabled={store.saving}
        onCancel={() => setRemoving(false)}
        onConfirm={() => {
          setRemoving(false);
          void store.remove(memory.id);
        }}
      />
    </li>
  );
});

function MemoryList({ memories }: { readonly memories: readonly MemoryDto[] }) {
  return (
    <ul className="m-0 list-none divide-y divide-line p-0">
      {memories.map((memory) => (
        <MemoryRow key={memory.id} memory={memory} />
      ))}
    </ul>
  );
}

const ClearButton = observer(function ClearButton({
  title,
  conversationId,
}: {
  readonly title: string;
  readonly conversationId?: ConversationId;
}) {
  const { memory } = useStore();
  return (
    <Button
      tone="ghost"
      disabled={memory.saving}
      needConfirm
      modalSetup={{ title, content: CLEAR_CONSEQUENCE, confirmLabel: CLEAR }}
      onClick={() => void memory.clear(conversationId)}
    >
      {CLEAR}
    </Button>
  );
});

const Composer = observer(function Composer({ scope }: { readonly scope: Scope }) {
  const { memory } = useStore();
  const [content, setContent] = useState("");
  const [target, setTarget] = useState("");
  const dialog = scope === "dialog";
  const conversations = memory.conversations;
  const chosen = conversations.find((conversation) => conversation.id === target)?.id;
  if (dialog && conversations.length === 0)
    return (
      <p className="m-0 rounded-card bg-main-850 px-4 py-3 text-[12px] text-main-500">
        {NO_DIALOGS}
      </p>
    );
  const ready = content.trim().length > 0 && (!dialog || chosen !== undefined);
  return (
    <TextComposer
      label={dialog ? DIALOG_PLACEHOLDER : ACCOUNT_PLACEHOLDER}
      placeholder={dialog ? DIALOG_PLACEHOLDER : ACCOUNT_PLACEHOLDER}
      value={content}
      max={MEMORY_CONTENT_MAX}
      submitLabel={ADD}
      canSubmit={ready}
      disabled={memory.saving}
      onChange={setContent}
      onSubmit={() => {
        void memory.add(content, dialog ? chosen : undefined).then((saved) => {
          if (saved) setContent("");
        });
      }}
      leading={
        dialog ? (
          <div className="w-60 min-w-0">
            <SelectInput
              label={DIALOG_PICKER}
              placeholder={DIALOG_PICKER}
              value={target}
              options={conversations.map((conversation) => ({
                value: conversation.id,
                label: conversation.title,
              }))}
              onChange={setTarget}
            />
          </div>
        ) : null
      }
    />
  );
});

const AccountFacts = observer(function AccountFacts() {
  const { memory } = useStore();
  if (memory.account.length === 0)
    return (
      <div className="flex justify-center">
        <EmptyState icon={mdiBrain} title={ACCOUNT_EMPTY_TITLE} description={ACCOUNT_EMPTY} />
      </div>
    );
  return (
    <div className="overflow-hidden rounded-card border border-line">
      <MemoryList memories={memory.account} />
    </div>
  );
});

const DialogFacts = observer(function DialogFacts() {
  const { memory } = useStore();
  if (memory.dialogs.length === 0)
    return (
      <div className="flex justify-center">
        <EmptyState icon={mdiForumOutline} title={DIALOG_EMPTY_TITLE} description={DIALOG_EMPTY} />
      </div>
    );
  return (
    <div className="flex flex-col gap-3">
      {memory.dialogs.map((group) => (
        <section
          key={group.conversation.id}
          className="overflow-hidden rounded-card border border-line"
        >
          <header className="flex items-center gap-2.5 border-b border-line bg-main-850 py-1.5 pr-1.5 pl-4">
            <Icon path={mdiForumOutline} size={15} className="flex-none text-main-400" />
            <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-main-100">
              {group.conversation.title}
            </span>
            <Chip>{factsLabel(group.memories.length)}</Chip>
            <ClearButton title={CLEAR_DIALOG_TITLE} conversationId={group.conversation.id} />
          </header>
          <MemoryList memories={group.memories} />
        </section>
      ))}
    </div>
  );
});

function MemoryFacts() {
  const { memory } = useStore();
  const [scope, setScope] = useState<Scope>("account");
  useEffect(() => {
    void memory.load();
  }, [memory]);
  if (!memory.loaded && memory.loading)
    return (
      <LoadingStatus label={LOADING} className="flex flex-col gap-2.5 py-6">
        <ListRowSkeleton />
        <ListRowSkeleton />
        <ListRowSkeleton />
      </LoadingStatus>
    );
  const dialogCount = memory.dialogs.reduce((sum, group) => sum + group.memories.length, 0);
  const options = [
    {
      value: "account" as const,
      label: `${ACCOUNT_TAB} · ${memory.account.length.toLocaleString("ru-RU")}`,
    },
    { value: "dialog" as const, label: `${DIALOG_TAB} · ${dialogCount.toLocaleString("ru-RU")}` },
  ];
  return (
    <div className="flex flex-col gap-4 pt-6">
      <div className="flex flex-wrap items-center gap-3">
        <SegmentedControl
          label={SCOPE_TABS_LABEL}
          value={scope}
          options={options}
          onChange={setScope}
        />
        <span className="min-w-0 flex-1 text-[12px] text-main-500">
          {scope === "account" ? ACCOUNT_HINT : DIALOG_HINT}
        </span>
        {scope === "account" && memory.account.length > 0 ? (
          <ClearButton title={CLEAR_ACCOUNT_TITLE} />
        ) : null}
      </div>
      {memory.error === null ? null : <Alert>{memory.error}</Alert>}
      <Composer key={scope} scope={scope} />
      {scope === "account" ? <AccountFacts /> : <DialogFacts />}
    </div>
  );
}

export default observer(MemoryFacts);
