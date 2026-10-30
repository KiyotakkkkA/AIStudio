export function appendDictation(current: string, text: string, max: number): string {
  const spoken = text.trim();
  if (spoken.length === 0) return current;
  const joined =
    current.length === 0 || /\s$/.test(current) ? `${current}${spoken}` : `${current} ${spoken}`;
  return joined.slice(0, max);
}
