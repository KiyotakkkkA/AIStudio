import type { Json } from "@zvs/shared";

export type JsonTokenKind = "key" | "string" | "number" | "literal" | "punctuation" | "space";

export interface JsonToken {
  readonly kind: JsonTokenKind;
  readonly text: string;
}

const TOKEN =
  /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|(true|false|null)|([{}[\],:])|(\s+)/g;

export function jsonTokens(value: Json): JsonToken[] {
  const text = JSON.stringify(value, null, 2);
  const tokens: JsonToken[] = [];
  for (const match of text.matchAll(TOKEN)) {
    const [whole, quoted, colon, number, literal, punctuation] = match;
    if (quoted !== undefined) {
      tokens.push({ kind: colon === undefined ? "string" : "key", text: quoted });
      if (colon !== undefined) tokens.push({ kind: "punctuation", text: colon });
    } else if (number !== undefined) {
      tokens.push({ kind: "number", text: number });
    } else if (literal !== undefined) {
      tokens.push({ kind: "literal", text: literal });
    } else if (punctuation !== undefined) {
      tokens.push({ kind: "punctuation", text: punctuation });
    } else {
      tokens.push({ kind: "space", text: whole });
    }
  }
  return tokens;
}
