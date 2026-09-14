function toRegExp(pattern: string): RegExp {
  let source = "";
  for (let i = 0; i < pattern.length; i += 1) {
    const char = pattern[i]!;
    if (char === "*") {
      if (pattern[i + 1] === "*") {
        i += 1;
        if (pattern[i + 1] === "/") i += 1;
        source += "(?:[^/]*(?:/|$))*";
      } else source += "[^/]*";
    } else if (char === "?") source += "[^/]";
    else source += char.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${source}$`, "i");
}

const cache = new Map<string, RegExp>();

export function normalizeRelative(path: string): string {
  return path.replaceAll("\\", "/").replace(/^\.?\//, "");
}

export function matchesPattern(relativePath: string, pattern: string): boolean {
  const key = pattern.replaceAll("\\", "/");
  let expression = cache.get(key);
  if (!expression) {
    expression = toRegExp(key.includes("/") ? key : `**/${key}`);
    cache.set(key, expression);
  }
  return expression.test(normalizeRelative(relativePath));
}

export function excludes(relativePath: string, exclude: readonly string[]): boolean {
  return exclude.some(
    (pattern) =>
      matchesPattern(relativePath, pattern) || matchesPattern(relativePath, `${pattern}/**`),
  );
}

export function accepts(
  relativePath: string,
  include: readonly string[],
  exclude: readonly string[],
): boolean {
  if (excludes(relativePath, exclude)) return false;
  if (include.length === 0) return true;
  return include.some((pattern) => matchesPattern(relativePath, pattern));
}
