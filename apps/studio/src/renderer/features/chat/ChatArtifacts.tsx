import { observer } from "mobx-react-lite";
import type { ArtifactDto } from "@zvs/shared";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiEyeOutline,
  mdiOpenInNew,
  mdiFolderOpenOutline,
  mdiTrayArrowDown,
  mdiWeb,
  mdiApplicationOutline,
} from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { useStore } from "../../stores/useStore";
import { viewerCanShow } from "../../viewer/viewerModel";
import Icon from "../../ui/atoms/Icon";
import type ArtifactActions from "./ArtifactActions";
import ArtifactTile from "./ArtifactTile";
import {
  ARTIFACTS_LABEL,
  DOWNLOAD_ARTIFACT,
  OPEN_ARTIFACT,
  OPEN_IN_BUILT_IN_BROWSER,
  OPEN_IN_VIEWER,
  REVEAL_ARTIFACT,
  VIEWER_UNAVAILABLE,
  artifactSubtitle,
  openInApplication,
  opensAsPage,
} from "./artifacts";

const MENU_ITEM = "text-[13px] text-main-100";

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
  const page = opensAsPage(artifact.format);
  return (
    <Dropdown
      menuWidth={280}
      menuPlacement="bottom-right"
      disabled={disabled}
      onOpenChange={(open) => {
        if (!open) return;
        void actions.loadApplication(artifact.id, artifact.format);
        if (page) void actions.loadBrowsers();
      }}
    >
      <IconDropdownButton
        label={OPEN_ARTIFACT}
        tooltip="top-center"
        path={mdiOpenInNew}
        iconSize={16}
        rounded="rounded-lg"
        disabled={disabled}
        className="size-8 justify-center gap-0 border-none cursor-auto bg-main-800 p-0 text-main-400 hover:bg-main-700 hover:text-main-50"
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
        {page
          ? actions.browsersFor(artifact.format).map((browser) => (
              <Dropdown.Item
                key={browser.id}
                role="menuitem"
                rounded="rounded-lg"
                className={MENU_ITEM}
                onClick={() => void actions.openInBrowser(artifact.id, browser.id)}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <Icon path={mdiWeb} size={17} className="flex-none text-main-400" />
                  <span className="truncate">{openInApplication(browser.name)}</span>
                </span>
              </Dropdown.Item>
            ))
          : null}
        {page ? (
          <Dropdown.Item
            role="menuitem"
            rounded="rounded-lg"
            className={MENU_ITEM}
            onClick={() => void actions.openInApp(artifact.id)}
          >
            <span className="flex items-center gap-2">
              <Icon path={mdiApplicationOutline} size={17} className="flex-none text-main-400" />
              {OPEN_IN_BUILT_IN_BROWSER}
            </span>
          </Dropdown.Item>
        ) : null}
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
        const busy = actions.busy[artifact.id];
        const error = actions.errors[artifact.id];
        const notice = actions.notices[artifact.id];
        return (
          <ArtifactTile
            key={artifact.id}
            name={artifact.name}
            format={artifact.format}
            subtitle={artifactSubtitle(artifact)}
            error={error}
            notice={notice}
            actions={
              <>
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
              </>
            }
          />
        );
      })}
    </ul>
  );
}

export default observer(ChatArtifacts);
