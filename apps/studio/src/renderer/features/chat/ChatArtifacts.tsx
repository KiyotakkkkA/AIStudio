import { observer } from "mobx-react-lite";
import type { ArtifactDto } from "@zvs/shared";
import {
  mdiFileDocumentOutline,
  mdiFileExcelOutline,
  mdiFileImageOutline,
  mdiFilePdfBox,
  mdiFilePowerpointOutline,
  mdiFileWordOutline,
  mdiFolderOpenOutline,
  mdiTrayArrowDown,
} from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import type ArtifactActions from "./ArtifactActions";
import {
  ARTIFACTS_LABEL,
  DOWNLOAD_ARTIFACT,
  REVEAL_ARTIFACT,
  artifactKind,
  artifactSubtitle,
} from "./artifacts";

const TILE_BACKGROUNDS: Record<StatusTone, string> = {
  ok: "bg-ok-soft",
  warn: "bg-warn-soft",
  err: "bg-err-soft",
  idle: "bg-main-700",
  accent: "bg-accent-soft",
};

function formatIcon(format: string): string {
  switch (format.toLowerCase()) {
    case "pdf":
      return mdiFilePdfBox;
    case "docx":
    case "doc":
    case "odt":
    case "rtf":
      return mdiFileWordOutline;
    case "xlsx":
    case "xlsm":
    case "xls":
    case "ods":
    case "csv":
      return mdiFileExcelOutline;
    case "pptx":
    case "ppt":
    case "odp":
      return mdiFilePowerpointOutline;
    case "png":
    case "jpg":
    case "jpeg":
    case "svg":
      return mdiFileImageOutline;
    default:
      return mdiFileDocumentOutline;
  }
}

function ChatArtifacts({
  artifacts,
  actions,
}: {
  readonly artifacts: readonly ArtifactDto[];
  readonly actions: ArtifactActions;
}) {
  if (artifacts.length === 0) return null;
  return (
    <ul aria-label={ARTIFACTS_LABEL} className="m-0 flex list-none flex-col gap-2 p-0">
      {artifacts.map((artifact) => {
        const tone = artifactKind(artifact.format).tone;
        const busy = actions.busy[artifact.id];
        const error = actions.errors[artifact.id];
        const notice = actions.notices[artifact.id];
        return (
          <li
            key={artifact.id}
            className="flex max-w-120 flex-col gap-1.5 rounded-card border border-line bg-main-900 px-3 py-2.5"
          >
            <div className="flex items-center gap-3">
              <span
                aria-hidden="true"
                className={`flex size-10 flex-none items-center justify-center rounded-control ${TILE_BACKGROUNDS[tone]} ${STATUS_TONE_TEXT[tone]}`}
              >
                <Icon path={formatIcon(artifact.format)} size={20} />
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span
                  className="truncate text-[13.5px] font-semibold text-main-50"
                  title={artifact.name}
                >
                  {artifact.name}
                </span>
                <span className="truncate text-[12px] text-main-500">
                  {artifactSubtitle(artifact)}
                </span>
              </span>
              <span className="flex flex-none items-center gap-1">
                <IconButton
                  path={mdiTrayArrowDown}
                  label={DOWNLOAD_ARTIFACT}
                  iconSize={16}
                  className="size-8 bg-main-800"
                  disabled={busy !== undefined}
                  onClick={() => void actions.save(artifact.id)}
                />
                <IconButton
                  path={mdiFolderOpenOutline}
                  label={REVEAL_ARTIFACT}
                  iconSize={16}
                  className="size-8 bg-main-800"
                  disabled={busy !== undefined}
                  onClick={() => void actions.reveal(artifact.id)}
                />
              </span>
            </div>
            {error === undefined ? null : (
              <p role="alert" className="m-0 text-[12px] text-err">
                {error}
              </p>
            )}
            {notice === undefined ? null : (
              <p role="status" className="m-0 text-[12px] text-ok">
                {notice}
              </p>
            )}
          </li>
        );
      })}
    </ul>
  );
}

export default observer(ChatArtifacts);
