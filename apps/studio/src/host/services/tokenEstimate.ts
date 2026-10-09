const CYRILLIC = /\p{Script=Cyrillic}/u;
const LATIN = /\p{Script=Latin}/u;
const DIGIT = /\p{Nd}/u;
const CJK = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u;
const SPACE = /\s/u;
const SYMBOL = /[\p{P}\p{S}]/u;

export const CHARS_PER_TOKEN = { cyrillic: 3, latin: 4 } as const;

export function estimateTokens(text: string): number {
  let cyrillic = 0;
  let latin = 0;
  let single = 0;
  let other = 0;
  for (const char of text) {
    if (CYRILLIC.test(char)) cyrillic += 1;
    else if (LATIN.test(char)) latin += 1;
    else if (DIGIT.test(char) || CJK.test(char) || SYMBOL.test(char)) single += 1;
    else if (!SPACE.test(char)) other += 1;
  }
  return Math.ceil(
    cyrillic / CHARS_PER_TOKEN.cyrillic + latin / CHARS_PER_TOKEN.latin + single + other * 2,
  );
}
