export const FOLLOW_UP_MAX_WORDS = 8;
export const FOLLOW_UP_CONTEXT_CHARS = 2000;

const OPENERS = new Set([
  "а",
  "и",
  "но",
  "или",
  "также",
  "тоже",
  "ещё",
  "еще",
  "тогда",
  "значит",
  "это",
  "этот",
  "эта",
  "эти",
  "этого",
  "этой",
  "этом",
  "этих",
  "тот",
  "та",
  "то",
  "те",
  "там",
  "тут",
  "он",
  "она",
  "оно",
  "они",
  "его",
  "её",
  "ее",
  "их",
  "ему",
  "ей",
  "им",
  "нём",
  "нем",
  "ней",
  "них",
  "такой",
  "такая",
  "такое",
  "такие",
  "and",
  "but",
  "or",
  "also",
  "so",
  "then",
  "it",
  "its",
  "this",
  "that",
  "these",
  "those",
  "they",
  "them",
  "their",
  "he",
  "she",
  "his",
  "her",
  "there",
  "what about",
]);

const ACKNOWLEDGEMENTS = new Set([
  "спасибо",
  "спс",
  "благодарю",
  "большое",
  "огромное",
  "ок",
  "окей",
  "хорошо",
  "ясно",
  "понятно",
  "понял",
  "поняла",
  "отлично",
  "супер",
  "класс",
  "да",
  "нет",
  "привет",
  "пока",
  "thanks",
  "thank",
  "you",
  "thx",
  "ok",
  "okay",
  "great",
  "cool",
  "got",
  "it",
  "yes",
  "no",
  "hi",
  "hello",
  "bye",
]);

function words(text: string): string[] {
  return text.toLowerCase().match(/[\p{L}\p{N}]+(?:[-'’][\p{L}\p{N}]+)*/gu) ?? [];
}

export function isFollowUp(text: string): boolean {
  const tokens = words(text);
  if (tokens.length === 0 || tokens.every((token) => ACKNOWLEDGEMENTS.has(token))) return false;
  if (tokens.length < FOLLOW_UP_MAX_WORDS) return true;
  return OPENERS.has(tokens[0]!) || OPENERS.has(`${tokens[0]!} ${tokens[1] ?? ""}`);
}

export function retrievalQuery(current: string, previousUser: string | undefined): string {
  const text = current.trim();
  const previous = previousUser?.trim().slice(0, FOLLOW_UP_CONTEXT_CHARS).trim() ?? "";
  if (previous === "" || !isFollowUp(text)) return text;
  return `${previous}\n${text}`;
}
