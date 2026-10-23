export const CSV_ROWS_MAX = 200_000;

export function csvDelimiter(firstLine: string, mimeType: string): string {
  if (mimeType.includes("tab-separated")) return "\t";
  const counts = [",", ";", "\t"].map((candidate) => ({
    candidate,
    count: firstLine.split(candidate).length - 1,
  }));
  const best = counts.reduce((left, right) => (right.count > left.count ? right : left));
  return best.count === 0 ? "," : best.candidate;
}

export function parseCsv(text: string, delimiter: string, limit = CSV_ROWS_MAX): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  let index = 0;
  const push = () => {
    row.push(field);
    field = "";
  };
  while (index < text.length && rows.length < limit) {
    const char = text[index]!;
    if (quoted) {
      if (char === '"') {
        if (text[index + 1] === '"') {
          field += '"';
          index += 2;
          continue;
        }
        quoted = false;
      } else {
        field += char;
      }
      index += 1;
      continue;
    }
    if (char === '"' && field.length === 0) {
      quoted = true;
    } else if (char === delimiter) {
      push();
    } else if (char === "\n" || char === "\r") {
      push();
      rows.push(row);
      row = [];
      if (char === "\r" && text[index + 1] === "\n") index += 1;
    } else {
      field += char;
    }
    index += 1;
  }
  if ((field.length > 0 || row.length > 0) && rows.length < limit) {
    push();
    rows.push(row);
  }
  return rows;
}

export function columnCount(rows: readonly (readonly string[])[]): number {
  return rows.reduce((max, row) => Math.max(max, row.length), 0);
}

export function columnName(index: number): string {
  let name = "";
  let value = index + 1;
  while (value > 0) {
    const rest = (value - 1) % 26;
    name = String.fromCharCode(65 + rest) + name;
    value = Math.floor((value - 1) / 26);
  }
  return name;
}

export function rowMatches(rows: readonly (readonly string[])[], query: string): number[] {
  const needle = query.trim().toLocaleLowerCase("ru-RU");
  if (needle.length === 0) return [];
  const found: number[] = [];
  rows.forEach((row, index) => {
    if (row.some((cell) => cell.toLocaleLowerCase("ru-RU").includes(needle))) found.push(index);
  });
  return found;
}

export const COLUMN_MIN_WIDTH = 72;
export const COLUMN_MAX_WIDTH = 320;
export const CHARACTER_WIDTH = 7.2;
export const CELL_PADDING = 20;
const WIDTH_SAMPLE = 1000;

export function columnWidths(rows: readonly (readonly string[])[], columns: number): number[] {
  const longest = Array.from({ length: columns }, () => 0);
  for (const row of rows.slice(0, WIDTH_SAMPLE)) {
    for (let column = 0; column < columns; column += 1) {
      const length = row[column]?.length ?? 0;
      if (length > longest[column]!) longest[column] = length;
    }
  }
  return longest.map((length) =>
    Math.round(
      Math.min(
        COLUMN_MAX_WIDTH,
        Math.max(COLUMN_MIN_WIDTH, length * CHARACTER_WIDTH + CELL_PADDING),
      ),
    ),
  );
}

export function isNumeric(cell: string): boolean {
  const compact = cell.trim().replace(/[\s\u00a0]/gu, "");
  return /^[-+]?\d+([.,]\d+)?%?$/u.test(compact);
}
