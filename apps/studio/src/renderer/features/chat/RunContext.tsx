import { useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiChevronDown, mdiMessageArrowRightOutline } from "@mdi/js";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import type ChatStore from "./ChatStore";
import SidePanelFrame from "./SidePanelFrame";
import { CONTEXT_PANEL } from "./explorer/explorerCopy";
import {
  ARTIFACTS_SECTION,
  NO_DIALOG_ARTIFACTS,
  SHOW_IN_CHAT,
  artifactSubtitle,
} from "./artifacts";

const DialogArtifacts = observer(function DialogArtifacts({
  store,
}: {
  readonly store: ChatStore;
}) {
  const [open, setOpen] = useState(true);
  const artifacts = store.dialogArtifacts;
  return (
    <section className="flex flex-col gap-2">
      <button
        type="button"
        aria-expanded={open}
        aria-controls="context-artifacts"
        onClick={() => setOpen(!open)}
        className="-mx-1.5 flex cursor-pointer items-center gap-2 rounded-control px-1.5 py-1 text-left text-main-500 hover:bg-main-700 hover:text-main-300"
      >
        <span className="min-w-0 flex-1 text-[10.5px] font-semibold tracking-[0.12em] uppercase">
          {ARTIFACTS_SECTION}
        </span>
        <Chip>{artifacts.length.toLocaleString("ru-RU")}</Chip>
        <Icon
          path={mdiChevronDown}
          size={15}
          className={`flex-none transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open ? (
        artifacts.length === 0 ? (
          <p
            id="context-artifacts"
            className="m-0 rounded-control bg-main-900 px-3 py-2 text-xs/snug text-main-500"
          >
            {NO_DIALOG_ARTIFACTS}
          </p>
        ) : (
          <ul
            id="context-artifacts"
            aria-label={ARTIFACTS_SECTION}
            className="m-0 flex list-none flex-col gap-1 p-0"
          >
            {artifacts.map(({ messageId, artifact }) => (
              <li
                key={artifact.id}
                className="flex items-center gap-2 rounded-control bg-main-900 py-1.5 pr-1 pl-3"
              >
                <span className="flex min-w-0 flex-1 flex-col">
                  <span
                    className="truncate text-[12.5px] font-medium text-main-100"
                    title={artifact.name}
                  >
                    {artifact.name}
                  </span>
                  <span className="truncate text-[11px] text-main-500">
                    {artifactSubtitle(artifact)}
                  </span>
                </span>
                <IconButton
                  aria-label={`${SHOW_IN_CHAT}: ${artifact.name}`}
                  label={SHOW_IN_CHAT}
                  tooltip="bottom-right"
                  path={mdiMessageArrowRightOutline}
                  iconSize={16}
                  onClick={() => store.showArtifact(messageId, artifact.id)}
                />
              </li>
            ))}
          </ul>
        )
      ) : null}
    </section>
  );
});

export default observer(function RunContext({ store }: { readonly store: ChatStore }) {
  return (
    <SidePanelFrame store={store} title={CONTEXT_PANEL}>
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-4 pt-1 pb-4.5">
        <DialogArtifacts store={store} />
      </ScrollArea>
    </SidePanelFrame>
  );
});
