const FENCE = /(```[\s\S]*?(?:```|$))/g;
const MARKER = /\[(\d{1,3})\](?![(:]|\[(?!\d{1,3}\]))/g;
const WEB_MARKER = /\[\[(\d{1,3})\]\]/g;
export const CITE_PREFIX = "#cite-";
export const WEB_PREFIX = "#web-";

function outsideFences(content: string, replace: (part: string) => string): string {
  return content
    .split(FENCE)
    .map((part) => (part.startsWith("```") ? part : replace(part)))
    .join("");
}

export function linkCitations(content: string, count: number): string {
  if (count === 0) return content;
  return outsideFences(content, (part) =>
    part.replace(MARKER, (marker, digits: string) => {
      const index = Number(digits);
      return index >= 1 && index <= count ? `[${digits}](${CITE_PREFIX}${digits})` : marker;
    }),
  );
}

export function linkWebSources(content: string, count: number): string {
  if (count === 0) return content;
  return outsideFences(content, (part) =>
    part.replace(WEB_MARKER, (marker, digits: string) => {
      const index = Number(digits);
      return index >= 1 && index <= count ? `[${digits}](${WEB_PREFIX}${digits})` : marker;
    }),
  );
}

function markerNumber(href: string | undefined, prefix: string): number | null {
  if (href?.startsWith(prefix) !== true) return null;
  const value = Number(href.slice(prefix.length));
  return Number.isInteger(value) && value > 0 ? value : null;
}

export function citationNumber(href: string | undefined): number | null {
  return markerNumber(href, CITE_PREFIX);
}

export function webSourceNumber(href: string | undefined): number | null {
  return markerNumber(href, WEB_PREFIX);
}
