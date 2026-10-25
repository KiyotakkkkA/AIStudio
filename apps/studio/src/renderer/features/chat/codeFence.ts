const CLOSING_FENCE = /\n[ \t]{0,3}(`{3,}|~{3,})[ \t]*$/u;

export function isFenceOpen(
  source: string,
  start: number | undefined,
  end: number | undefined,
): boolean {
  if (start === undefined || end === undefined) return false;
  if (source.slice(end).trim().length > 0) return false;
  const block = source.slice(start, end).trimEnd();
  return !CLOSING_FENCE.test(block);
}
