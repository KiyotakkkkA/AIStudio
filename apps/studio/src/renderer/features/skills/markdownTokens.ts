export type TokenKind =
  "plain" | "heading" | "marker" | "code" | "strong" | "emphasis" | "fence" | "quote";

export interface Token {
  readonly kind: TokenKind;
  readonly text: string;
}

const INLINE = /(`[^`\n]+`|\*\*[^*\n]+\*\*|__[^_\n]+__|(?<![*\w])\*[^*\s][^*\n]*\*(?!\*))/g;
const LIST = /^(\s*)([-*+]|\d+[.)])(\s+)/;
const HEADING = /^#{1,6}\s/;
const FENCE = /^\s*(```|~~~)/;

function inline(text: string): Token[] {
  const tokens: Token[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const value = match[0];
    const at = match.index;
    if (at > last) tokens.push({ kind: "plain", text: text.slice(last, at) });
    const kind: TokenKind = value.startsWith("`")
      ? "code"
      : value.startsWith("**") || value.startsWith("__")
        ? "strong"
        : "emphasis";
    tokens.push({ kind, text: value });
    last = at + value.length;
  }
  if (last < text.length) tokens.push({ kind: "plain", text: text.slice(last) });
  return tokens;
}

export function markdownLines(source: string): Token[][] {
  const lines = source.split("\n");
  let fenced = false;
  return lines.map((line) => {
    if (FENCE.test(line)) {
      fenced = !fenced;
      return [{ kind: "fence", text: line }];
    }
    if (fenced) return [{ kind: "code", text: line }];
    if (HEADING.test(line)) return [{ kind: "heading", text: line }];
    if (line.trimStart().startsWith(">")) return [{ kind: "quote", text: line }];
    const list = LIST.exec(line);
    if (list !== null) {
      const [whole, indent = "", marker = "", gap = ""] = list;
      return [
        ...(indent.length > 0 ? [{ kind: "plain" as const, text: indent }] : []),
        { kind: "marker", text: marker },
        { kind: "plain", text: gap },
        ...inline(line.slice(whole.length)),
      ];
    }
    return inline(line);
  });
}
