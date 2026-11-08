import { SiteIconDto } from "@zvs/shared";

export async function loadBookmarkIcon(
  id: string,
  call: (channel: string, payload: unknown) => Promise<unknown>,
): Promise<Blob | null> {
  try {
    const result = await call("browser.command", { action: "bookmarkIcon", id });
    const parsed = SiteIconDto.safeParse(result);
    if (!parsed.success) return null;
    const bytes = Uint8Array.from(atob(parsed.data.data), (char) => char.charCodeAt(0));
    return new Blob([bytes], { type: parsed.data.mime });
  } catch {
    return null;
  }
}
