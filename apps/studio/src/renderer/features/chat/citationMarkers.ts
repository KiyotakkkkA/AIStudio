const FENCE = /(```[\s\S]*?(?:```|$))/g;
const MARKER = /\[(\d{1,3})\](?![(:]|\[(?!\d{1,3}\]))/g;
export const CITE_PREFIX = "#cite-";

export function linkCitations(content: string, count: number): string {
  if (count === 0) return content;
  return content
    .split(FENCE)
    .map((part) =>
      part.startsWith("```")
        ? part
        : part.replace(MARKER, (marker, digits: string) => {
            const index = Number(digits);
            return index >= 1 && index <= count ? `[${digits}](${CITE_PREFIX}${digits})` : marker;
          }),
    )
    .join("");
}

export function citationNumber(href: string | undefined): number | null {
  if (href?.startsWith(CITE_PREFIX) !== true) return null;
  const value = Number(href.slice(CITE_PREFIX.length));
  return Number.isInteger(value) && value > 0 ? value : null;
}
