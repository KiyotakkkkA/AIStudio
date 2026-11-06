export function displayAddress(url: string, full: boolean): string {
  if (full || url.length === 0) return url;
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return url;
    return parsed.host;
  } catch {
    return url;
  }
}
