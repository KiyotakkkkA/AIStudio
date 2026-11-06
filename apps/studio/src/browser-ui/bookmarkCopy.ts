import type { BrowserBookmarkLink } from "@zvs/shared";

export const BOOKMARK_ADD = "Добавить в закладки";
export const BOOKMARK_REMOVE = "Удалить из закладок";
export const BOOKMARKS_TITLE = "Закладки";

export function bookmarkLabel(bookmark: BrowserBookmarkLink): string {
  if (bookmark.title.length > 0 && bookmark.title !== bookmark.url) return bookmark.title;
  try {
    return new URL(bookmark.url).host;
  } catch {
    return bookmark.url;
  }
}
