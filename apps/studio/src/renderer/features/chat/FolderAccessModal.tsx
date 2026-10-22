import { observer } from "mobx-react-lite";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import type ChatStore from "./ChatStore";
import {
  ALLOW_ALWAYS,
  ALLOW_CONVERSATION,
  ALLOW_ONCE,
  CREATES_LABEL,
  DENY,
  SUBFOLDERS_NOTE,
  accessLead,
  accessTitle,
} from "./workingFolderCopy";

export default observer(function FolderAccessModal({ store }: { readonly store: ChatStore }) {
  const request = store.folderRequest;
  return (
    <ConfirmModal
      open={request !== null}
      title={request === null ? "" : accessTitle(request)}
      tone="primary"
      cancelLabel={DENY}
      disabled={store.answeringFolder}
      content={
        request === null ? null : (
          <div className="space-y-3">
            <p>{accessLead(request)}</p>
            <ul className="space-y-2">
              {request.folders.map((folder) => (
                <li key={folder.path} className="rounded-control bg-main-850 px-3 py-2">
                  <p className="font-mono text-[12.5px] break-all text-main-100">{folder.path}</p>
                  {folder.creates.length === 0 ? null : (
                    <div className="mt-1 text-[12px] text-main-400">
                      <span>{CREATES_LABEL}</span>
                      <ul className="mt-0.5 space-y-0.5 font-mono">
                        {folder.creates.map((name) => (
                          <li key={name} className="break-all">
                            {name}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </li>
              ))}
            </ul>
            <p>{SUBFOLDERS_NOTE}</p>
          </div>
        )
      }
      actions={[
        { label: ALLOW_ONCE, tone: "secondary", onSelect: () => void store.answerFolder("once") },
        {
          label: ALLOW_CONVERSATION,
          tone: "secondary",
          onSelect: () => void store.answerFolder("conversation"),
        },
        { label: ALLOW_ALWAYS, onSelect: () => void store.answerFolder("always") },
      ]}
      onCancel={() => void store.answerFolder("deny")}
    />
  );
});
