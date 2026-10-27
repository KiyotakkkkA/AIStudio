export const PINNED_GROUP = "Закреплённые";
export const PIN_CONVERSATION = "Закрепить";
export const UNPIN_CONVERSATION = "Открепить";
export const PINS_PANEL = "Закреплённые сообщения";
export const PINS_NAVIGATION = "Навигация по закреплённым сообщениям";
export const EMPTY_PIN = "Ответ без текста";

const FENCE = /^(`{3,}|~{3,})/;

export function pinPreview(content: string): string {
  let fenced = false;
  let code = "";
  for (const raw of content.split("\n")) {
    const line = raw.trim();
    if (FENCE.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) {
      code ||= line;
      continue;
    }
    const text = line
      .replace(/^(#{1,6}\s+|>\s*|[-*+]\s+|\d+[.)]\s+)+/, "")
      .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/\[\[\d+\]\]/g, "")
      .replace(/(\*\*|__|\*|_|~~|`)/g, "")
      .replace(/\s+/g, " ")
      .trim();
    if (text) return text;
  }
  return code || EMPTY_PIN;
}

export function currentPin(
  tops: readonly { readonly id: string; readonly top: number }[],
  threshold: number,
): string | null {
  let current = tops[0]?.id ?? null;
  for (const item of tops) {
    if (item.top > threshold) break;
    current = item.id;
  }
  return current;
}
