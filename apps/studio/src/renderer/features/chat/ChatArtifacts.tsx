import { observer } from "mobx-react-lite";
import type { ArtifactDto } from "@zvs/shared";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiEyeOutline,
  mdiOpenInNew,
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
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { useStore } from "../../stores/useStore";
import { viewerCanShow } from "../../viewer/viewerModel";
import Icon from "../../ui/atoms/Icon";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import type ArtifactActions from "./ArtifactActions";
import {
  ARTIFACTS_LABEL,
  DOWNLOAD_ARTIFACT,
  OPEN_ARTIFACT,
  OPEN_IN_VIEWER,
  REVEAL_ARTIFACT,
  VIEWER_UNAVAILABLE,
  artifactKind,
  artifactSubtitle,
  openInApplication,
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

const OpenMenu = observer(function OpenMenu({
  artifact,
  actions,
  disabled,
}: {
  readonly artifact: ArtifactDto;
  readonly actions: ArtifactActions;
  readonly disabled: boolean;
}) {
  const { viewer } = useStore();
  const viewable = viewerCanShow(artifact.format);
  return (
    <Dropdown
      menuWidth={260}
      menuPlacement="bottom-right"
      disabled={disabled}
      onOpenChange={(open) => {
        if (open) void actions.loadApplication(artifact.id, artifact.format);
      }}
    >
      <IconDropdownButton
        label={OPEN_ARTIFACT}
        tooltip="top-center"
        path={mdiOpenInNew}
        iconSize={16}
        rounded="rounded-lg"
        disabled={disabled}
        className="size-8 justify-center gap-0 border-transparent bg-main-800 p-0 text-main-400 hover:bg-main-700 hover:text-main-50"
      />
      <Dropdown.Menu
        role="menu"
        aria-label={OPEN_ARTIFACT}
        rounded="rounded-lg"
        className="rounded-card border-line bg-main-900 p-1 shadow-pop"
      >
        <Dropdown.Item
          role="menuitem"
          rounded="rounded-lg"
          className="text-[13px] text-main-100"
          onClick={() => void actions.open(artifact.id)}
        >
          <span className="flex min-w-0 items-center gap-2">
            <Icon path={mdiOpenInNew} size={17} className="flex-none text-main-400" />
            <span className="truncate">
              {openInApplication(actions.applicationOf(artifact.format))}
            </span>
          </span>
        </Dropdown.Item>
        <Dropdown.Item
          role="menuitem"
          rounded="rounded-lg"
          disabled={!viewable}
          title={viewable ? undefined : VIEWER_UNAVAILABLE}
          className="text-[13px] text-main-100 disabled:opacity-45"
          onClick={() => void viewer.openArtifact(artifact)}
        >
          <span className="flex items-center gap-2">
            <Icon path={mdiEyeOutline} size={17} className="flex-none text-main-400" />
            {OPEN_IN_VIEWER}
          </span>
        </Dropdown.Item>
      </Dropdown.Menu>
    </Dropdown>
  );
});

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
                <OpenMenu artifact={artifact} actions={actions} disabled={busy !== undefined} />
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
