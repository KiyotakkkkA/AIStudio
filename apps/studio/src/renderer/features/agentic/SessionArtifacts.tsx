import { observer } from "mobx-react-lite";
import { mdiEyeOutline, mdiFolderOpenOutline } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import { useStore } from "../../stores/useStore";
import type ViewerStore from "../../stores/ViewerStore";
import { extensionOf, joinFolderPath, opensInViewer } from "../../viewer/viewerModel";
import ArtifactTile from "../chat/ArtifactTile";
import { ARTIFACTS_LABEL, OPEN_ARTIFACT, REVEAL_ARTIFACT, artifactKind } from "../chat/artifacts";
import type AgenticStore from "./AgenticStore";
import { sessionArtifacts } from "./sessionModel";

function nameOf(path: string): string {
  return path.split("/").at(-1) ?? path;
}

function folderOf(path: string): string | null {
  const at = path.lastIndexOf("/");
  return at === -1 ? null : path.slice(0, at);
}

function artifactLine(path: string): string {
  const format = extensionOf(path);
  return [
    artifactKind(format).label,
    ...(format.length === 0 ? [] : [format.toUpperCase()]),
    ...(folderOf(path) === null ? [] : [folderOf(path) ?? ""]),
  ].join(" · ");
}

async function openArtifact(store: AgenticStore, viewer: ViewerStore, path: string) {
  const workspace = store.workingFolder;
  if (workspace === null) return;
  if (opensInViewer(nameOf(path))) {
    store.explorer.closeFile();
    await viewer.openPath(joinFolderPath(workspace, path), path);
    return;
  }
  viewer.close();
  await store.revealInExplorer(path);
  await store.explorer.open(path);
}

export default observer(function SessionArtifacts({ store }: { readonly store: AgenticStore }) {
  const { viewer } = useStore();
  const paths = sessionArtifacts(store.stream.entries, store.session?.summary ?? null);
  if (paths.length === 0) return null;
  return (
    <ul aria-label={ARTIFACTS_LABEL} className="m-0 flex list-none flex-col gap-2 p-0">
      {paths.map((path) => (
        <ArtifactTile
          key={path}
          name={nameOf(path)}
          format={extensionOf(path)}
          subtitle={artifactLine(path)}
          actions={
            <>
              <IconButton
                path={mdiEyeOutline}
                label={OPEN_ARTIFACT}
                iconSize={16}
                className="size-8 bg-main-800"
                onClick={() => void openArtifact(store, viewer, path)}
              />
              <IconButton
                path={mdiFolderOpenOutline}
                label={REVEAL_ARTIFACT}
                iconSize={16}
                className="size-8 bg-main-800"
                onClick={() => void store.revealInExplorer(path)}
              />
            </>
          }
        />
      ))}
    </ul>
  );
});
