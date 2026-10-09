export function splitArgs(text: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let started = false;
  for (let index = 0; index < text.length; index++) {
    const char = text[index]!;
    if (quote !== null) {
      if (char === quote) {
        quote = null;
      } else if (
        char === "\\" &&
        quote === '"' &&
        (text[index + 1] === '"' || text[index + 1] === "\\")
      ) {
        current += text[index + 1]!;
        index++;
      } else {
        current += char;
      }
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      started = true;
      continue;
    }
    if (/\s/.test(char)) {
      if (started) args.push(current);
      current = "";
      started = false;
      continue;
    }
    current += char;
    started = true;
  }
  if (started) args.push(current);
  return args;
}

export function joinArgs(args: readonly string[]): string {
  return args
    .map((arg) => {
      if (arg.length > 0 && !/[\s"']/.test(arg)) return arg;
      return `"${arg.replace(/(["\\])/g, "\\$1")}"`;
    })
    .join(" ");
}

export function hasUnclosedQuote(text: string): boolean {
  let quote: string | null = null;
  for (let index = 0; index < text.length; index++) {
    const char = text[index]!;
    if (quote !== null) {
      if (char === "\\" && quote === '"') index++;
      else if (char === quote) quote = null;
    } else if (char === '"' || char === "'") {
      quote = char;
    }
  }
  return quote !== null;
}
