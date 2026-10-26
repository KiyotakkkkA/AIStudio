import { observer } from "mobx-react-lite";
import { useState } from "react";
import { Dropdown, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiAccountOutline,
  mdiBrain,
  mdiCogOutline,
  mdiCreationOutline,
  mdiForumOutline,
} from "@mdi/js";
import type { ConversationId, MemoryDto } from "@zvs/shared";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";
import {
  ENABLED_ROW,
  LOADING,
  MEMORY_PANEL,
  PANEL_ACCOUNT,
  PANEL_DIALOG,
  PANEL_EMPTY,
  PANEL_MANAGE,
  PANEL_OFF,
  SOURCE_MODEL,
  SOURCE_USER,
} from "./memoryCopy";
import { LoadingStatus, SkeletonLines } from "../../ui/molecules/LoadingSkeleton";

const TRIGGER_OPEN =
  "bg-accent-soft text-accent-medium hover:bg-accent-soft hover:text-accent-light";

function FactRow({ memory }: { readonly memory: MemoryDto }) {
  const byModel = memory.source === "model";
  return (
    <li className="flex items-center gap-2.5 px-3 py-2">
      <span
        title={byModel ? SOURCE_MODEL : SOURCE_USER}
        className={`flex size-5 flex-none items-center justify-center rounded-[6px] ${
          byModel ? "bg-accent-soft text-accent-medium" : "bg-main-700 text-main-300"
        }`}
      >
        <Icon path={byModel ? mdiCreationOutline : mdiAccountOutline} size={12} />
      </span>
      <span className="min-w-0 flex-1 text-[12.5px] leading-normal wrap-break-word text-main-100">
        {memory.content}
      </span>
    </li>
  );
}

function FactSection({
  title,
  icon,
  memories,
}: {
  readonly title: string;
  readonly icon: string;
  readonly memories: readonly MemoryDto[];
}) {
  return (
    <section className="flex flex-col">
      <h3 className="m-0 flex items-center gap-2 px-3 pt-3 pb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        <Icon path={icon} size={13} className="flex-none" />
        <span className="min-w-0 flex-1 truncate">{title}</span>
        <Chip>{memories.length.toLocaleString("ru-RU")}</Chip>
      </h3>
      {memories.length === 0 ? (
        <p className="m-0 px-3 pb-2 text-[12px] text-main-500">{PANEL_EMPTY}</p>
      ) : (
        <ul className="m-0 list-none divide-y divide-line p-0">
          {memories.map((memory) => (
            <FactRow key={memory.id} memory={memory} />
          ))}
        </ul>
      )}
    </section>
  );
}

function MemoryDropdown({ conversationId }: { readonly conversationId: ConversationId | null }) {
  const { memory, appSettings, ui } = useStore();
  const [open, setOpen] = useState(false);
  const enabled = appSettings.values[ENABLED_ROW.key];
  return (
    <Dropdown
      menuWidth={340}
      menuPlacement="bottom-right"
      onOpenChange={(next) => {
        setOpen(next);
        if (next) {
          void memory.load();
          void appSettings.load();
        }
      }}
    >
      <IconDropdownButton
        label={MEMORY_PANEL}
        tooltip="bottom-center"
        path={mdiBrain}
        iconSize={16}
        size="sm"
        rounded=""
        aria-pressed={open}
        className="rounded-control"
        {...(open ? { toneClassName: TRIGGER_OPEN } : {})}
      />
      <Dropdown.Menu
        aria-label={MEMORY_PANEL}
        rounded=""
        className="overflow-hidden rounded-card border-line bg-main-900 p-0 shadow-pop"
      >
        <header className="flex items-center gap-2 border-b border-line px-3 py-2.5">
          <Icon path={mdiBrain} size={15} className="flex-none text-accent-medium" />
          <span className="min-w-0 flex-1 text-[13px] font-semibold text-main-50">
            {MEMORY_PANEL}
          </span>
        </header>
        {enabled ? null : (
          <p className="m-0 border-b border-line bg-warn-soft px-3 py-2 text-[12px] text-warn">
            {PANEL_OFF}
          </p>
        )}
        <ScrollArea className="max-h-90">
          {!memory.loaded && memory.loading ? (
            <LoadingStatus label={LOADING} className="px-3 py-4">
              <SkeletonLines count={4} />
            </LoadingStatus>
          ) : (
            <div className="flex flex-col pb-1.5">
              <FactSection
                title={PANEL_ACCOUNT}
                icon={mdiAccountOutline}
                memories={memory.account}
              />
              {conversationId === null ? null : (
                <FactSection
                  title={PANEL_DIALOG}
                  icon={mdiForumOutline}
                  memories={memory.conversationFacts(conversationId)}
                />
              )}
            </div>
          )}
        </ScrollArea>
        <div className="border-t border-line p-1">
          <Dropdown.Item
            rounded=""
            className="rounded-control text-[12.5px] text-main-300"
            icon={<Icon path={mdiCogOutline} size={15} className="text-main-400" />}
            onClick={() => ui.openPreferences("memory")}
          >
            {PANEL_MANAGE}
          </Dropdown.Item>
        </div>
      </Dropdown.Menu>
    </Dropdown>
  );
}

export default observer(MemoryDropdown);
