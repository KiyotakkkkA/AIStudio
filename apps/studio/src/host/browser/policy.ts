export const BROWSER_PARTITION = "persist:browser-work";
export const CHROME_HEIGHT = 136;
export const STATUS_HEIGHT = 28;

export function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password;
  } catch {
    return false;
  }
}

export function navigationUrl(input: string): string {
  const text = input.trim();
  const candidate =
    /^[a-z][a-z\d+.-]*:/i.test(text) && !/^[\w.-]+:\d+(?:\/|$)/.test(text)
      ? text
      : `https://${text}`;
  if (!isWebUrl(candidate)) throw new Error("Only HTTP and HTTPS addresses are allowed");
  return new URL(candidate).href;
}
