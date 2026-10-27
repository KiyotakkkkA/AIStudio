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
import { ActionMenu, ActionMenuItem } from "../../ui/molecules/ActionMenu";
import { useStore } from "../../stores/useStore";
import { viewerCanShow } from "../../viewer/viewerModel";
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
        disabled={disabled}
        toneClassName="bg-main-800 text-main-400 hover:bg-main-700 hover:text-main-50"
      />
      <ActionMenu label={OPEN_ARTIFACT}>
        <ActionMenuItem icon={mdiOpenInNew} onClick={() => void actions.open(artifact.id)}>
          {openInApplication(actions.applicationOf(artifact.format))}
        </ActionMenuItem>
        {page
          ? actions.browsersFor(artifact.format).map((browser) => (
              <ActionMenuItem
                key={browser.id}
                icon={mdiWeb}
                onClick={() => void actions.openInBrowser(artifact.id, browser.id)}
              >
                {openInApplication(browser.name)}
              </ActionMenuItem>
            ))
          : null}
        {page ? (
          <ActionMenuItem
            icon={mdiApplicationOutline}
            onClick={() => void actions.openInApp(artifact.id)}
          >
            {OPEN_IN_BUILT_IN_BROWSER}
          </ActionMenuItem>
        ) : null}
        <ActionMenuItem
          icon={mdiEyeOutline}
          disabled={!viewable}
          {...(viewable ? {} : { title: VIEWER_UNAVAILABLE })}
          onClick={() => void viewer.openArtifact(artifact)}
        >
          {OPEN_IN_VIEWER}
        </ActionMenuItem>
      </ActionMenu>
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
            anchor={artifact.id}
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
