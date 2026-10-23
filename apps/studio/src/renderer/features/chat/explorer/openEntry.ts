import type { ExplorerEntryDto } from "@zvs/shared";
import type ViewerStore from "../../../stores/ViewerStore";
import { joinFolderPath, opensInViewer } from "../../../viewer/viewerModel";
import type ExplorerStore from "./ExplorerStore";

export function viewedPath(viewer: ViewerStore, folder: string | null): string | null {
  const target = viewer.target;
  if (folder === null || target === null || target.ref.source !== "path") return null;
  return target.location;
}

export async function openEntry(
  explorer: ExplorerStore,
  viewer: ViewerStore,
  folder: string | null,
  entry: ExplorerEntryDto,
): Promise<void> {
  if (entry.kind !== "folder" && folder !== null && opensInViewer(entry.name)) {
    explorer.focus(entry.path);
    explorer.closeFile();
    await viewer.openPath(joinFolderPath(folder, entry.path), entry.path);
    return;
  }
  if (entry.kind !== "folder") viewer.close();
  await explorer.activate(entry);
}
