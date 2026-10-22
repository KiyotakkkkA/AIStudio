import type { PendingFile } from "./SkillFormVm";
import type { SkillFileDir } from "./skillPresentation";

function base64Of(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const chunk = 0x8000;
  for (let at = 0; at < bytes.length; at += chunk) {
    binary += String.fromCharCode(...bytes.subarray(at, at + chunk));
  }
  return btoa(binary);
}

export async function readFiles(files: Iterable<File>, dir: SkillFileDir): Promise<PendingFile[]> {
  return Promise.all(
    [...files].map(async (file) => ({
      path: `${dir}/${file.name}`,
      size: file.size,
      content: base64Of(await file.arrayBuffer()),
    })),
  );
}
