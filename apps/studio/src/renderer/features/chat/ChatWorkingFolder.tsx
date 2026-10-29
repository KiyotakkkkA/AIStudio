import { mdiClose, mdiFolderOutline, mdiFolderSwapOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import type ChatStore from "./ChatStore";
import {
  ALLOW_ALWAYS,
  ALLOW_CONVERSATION,
  BIND_TITLE,
  CANCEL,
  CHANGE_FOLDER,
  CHOOSE_FOLDER,
  FOLDER_ACCESS_MISSING,
  FOLDER_LOCKED,
  REMOVE_FOLDER,
  RESTORE_FOLDER_ACCESS,
  SUBFOLDERS_NOTE,
  WORKING_FOLDER_LABEL,
  folderName,
} from "./workingFolderCopy";

export default observer(function ChatWorkingFolder({ store }: { readonly store: ChatStore }) {
  const folder = store.workingFolder;
  const locked = !store.canChangeFolder;
  const hint = store.generating ? FOLDER_LOCKED : undefined;

  return (
    <div className="-mb-1 flex flex-col gap-1.5">
      <div
        role="group"
        aria-label={WORKING_FOLDER_LABEL}
        className="flex min-w-0 items-center gap-2 text-[12px] text-main-400"
      >
        {folder === null ? (
          <button
            type="button"
            disabled={!store.canPickFolder}
            title={hint}
            onClick={() => void store.pickFolder()}
            className="inline-flex h-7 items-center gap-1.5 rounded-control px-2 text-main-400 hover:bg-main-800 hover:text-main-100 disabled:pointer-events-none disabled:opacity-50"
          >
            <Icon path={mdiFolderOutline} size={14} />
            {CHOOSE_FOLDER}
          </button>
        ) : (
          <>
            <span
              className="inline-flex z-0 h-7 min-w-0 items-center gap-1.5 rounded-control bg-main-800 px-2 text-main-200"
              title={folder}
            >
              <Icon path={mdiFolderOutline} size={14} />
              <span className="truncate font-medium">{folderName(folder)}</span>
            </span>
            {store.workingFolderAccess === "missing" ? (
              <>
                <span className="inline-flex h-5 items-center rounded-full bg-warn-soft px-2 text-[11px] font-medium text-warn">
                  {FOLDER_ACCESS_MISSING}
                </span>
                <Chip onClick={locked ? undefined : () => void store.restoreFolderAccess()}>
                  {RESTORE_FOLDER_ACCESS}
                </Chip>
              </>
            ) : null}
            <IconButton
              aria-label={CHANGE_FOLDER}
              label={hint ?? CHANGE_FOLDER}
              tooltip="top-center"
              path={mdiFolderSwapOutline}
              disabled={locked}
              onClick={() => void store.pickFolder()}
            />
            <IconButton
              aria-label={REMOVE_FOLDER}
              label={hint ?? REMOVE_FOLDER}
              tooltip="top-center"
              path={mdiClose}
              disabled={locked}
              onClick={() => void store.unbindFolder()}
            />
          </>
        )}
      </div>
      {store.folderError === null ? null : (
        <p role="alert" className="text-[12px] text-err">
          {store.folderError}
        </p>
      )}
      <ConfirmModal
        open={store.folderPrompt !== null}
        title={BIND_TITLE}
        tone="primary"
        cancelLabel={CANCEL}
        disabled={store.folderBusy}
        content={
          <div className="space-y-2">
            <p className="font-mono text-[12.5px] break-all text-main-100">
              {store.folderPrompt?.path}
            </p>
            <p>{SUBFOLDERS_NOTE}</p>
          </div>
        }
        actions={[
          {
            label: ALLOW_CONVERSATION,
            tone: "secondary",
            onSelect: () => void store.confirmFolder("conversation"),
          },
          { label: ALLOW_ALWAYS, onSelect: () => void store.confirmFolder("always") },
        ]}
        onCancel={store.cancelFolderPrompt}
      />
    </div>
  );
});
