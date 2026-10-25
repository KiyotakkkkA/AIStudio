import { observer } from "mobx-react-lite";
import { mdiEyeOutline, mdiFolderOpenOutline } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import { useStore } from "../../stores/useStore";
import type ViewerStore from "../../stores/ViewerStore";
import { extensionOf, opensInViewer } from "../../viewer/viewerModel";
import ArtifactTile from "../chat/ArtifactTile";
import { ARTIFACTS_LABEL, OPEN_ARTIFACT, REVEAL_ARTIFACT, artifactKind } from "../chat/artifacts";
import type AgenticStore from "./AgenticStore";
import { resolveArtifact, sessionArtifacts, type ArtifactFile } from "./sessionModel";

function nameOf(path: string): string {
  return path.split(/[\\/]/u).at(-1) ?? path;
}

function folderOf(path: string): string | null {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return at <= 0 ? null : path.slice(0, at);
}

function artifactLine(file: ArtifactFile): string {
  const format = extensionOf(file.absolute);
  const folder = folderOf(file.relative ?? file.absolute);
  return [
    artifactKind(format).label,
    ...(format.length === 0 ? [] : [format.toUpperCase()]),
    ...(folder === null ? [] : [folder]),
  ].join(" · ");
}

async function openArtifact(store: AgenticStore, viewer: ViewerStore, file: ArtifactFile) {
  if (file.relative === null || opensInViewer(nameOf(file.absolute))) {
    store.explorer.closeFile();
    await viewer.openPath(file.absolute, file.relative ?? file.absolute);
    return;
  }
  viewer.close();
  await store.revealInExplorer(file.relative);
  await store.explorer.open(file.relative);
}

export default observer(function SessionArtifacts({ store }: { readonly store: AgenticStore }) {
  const { viewer } = useStore();
  const workspace = store.workingFolder;
  if (workspace === null) return null;
  const files = new Map<string, ArtifactFile>();
  for (const path of sessionArtifacts(store.stream.entries, store.session?.summary ?? null)) {
    const file = resolveArtifact(path, workspace);
    files.set(file.absolute.toLowerCase(), file);
  }
  if (files.size === 0) return null;
  return (
    <ul aria-label={ARTIFACTS_LABEL} className="m-0 flex list-none flex-col gap-2 p-0">
      {[...files.values()].map((file) => {
        const relative = file.relative;
        return (
          <ArtifactTile
            key={file.absolute}
            name={nameOf(file.absolute)}
            format={extensionOf(file.absolute)}
            subtitle={artifactLine(file)}
            actions={
              <>
                <IconButton
                  path={mdiEyeOutline}
                  label={OPEN_ARTIFACT}
                  iconSize={16}
                  className="size-8 bg-main-800"
                  onClick={() => void openArtifact(store, viewer, file)}
                />
                {relative === null ? null : (
                  <IconButton
                    path={mdiFolderOpenOutline}
                    label={REVEAL_ARTIFACT}
                    iconSize={16}
                    className="size-8 bg-main-800"
                    onClick={() => void store.revealInExplorer(relative)}
                  />
                )}
              </>
            }
          />
        );
      })}
    </ul>
  );
});
