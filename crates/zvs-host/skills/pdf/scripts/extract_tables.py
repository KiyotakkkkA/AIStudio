# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber==0.11.10"]
# ///
import argparse
import csv
import io
import sys
from pathlib import Path

import pdfplumber


def parse_pages(spec: str | None, count: int) -> list[int]:
    if not spec:
        return list(range(1, count + 1))
    pages: list[int] = []
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, _, end = part.partition("-")
            first = int(start) if start else 1
            last = int(end) if end else count
            pages.extend(range(first, last + 1))
        else:
            pages.append(int(part))
    return [page for page in dict.fromkeys(pages) if 1 <= page <= count]


def clean(cell: object) -> str:
    return " ".join(str(cell).split()) if cell is not None else ""


def markdown(rows: list[list[str]]) -> str:
    width = max(len(row) for row in rows)
    padded = [row + [""] * (width - len(row)) for row in rows]
    escape = lambda value: value.replace("|", "\\|")
    lines = ["| " + " | ".join(escape(cell) for cell in padded[0]) + " |"]
    lines.append("|" + " --- |" * width)
    for row in padded[1:]:
        lines.append("| " + " | ".join(escape(cell) for cell in row) + " |")
    return "\n".join(lines)


def to_csv(rows: list[list[str]]) -> str:
    buffer = io.StringIO()
    csv.writer(buffer).writerows(rows)
    return buffer.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser(description="Extract tables from a PDF.")
    parser.add_argument("file")
    parser.add_argument("--pages", help="1-based pages, e.g. 2,4-6")
    parser.add_argument("--format", choices=["markdown", "csv"], default="markdown")
    parser.add_argument("--out-dir", help="write one CSV file per table into this folder")
    parser.add_argument("--text-strategy", action="store_true", help="detect tables without ruling lines")
    parser.add_argument("--password", default=None)
    args = parser.parse_args()

    settings = {"vertical_strategy": "text", "horizontal_strategy": "text"} if args.text_strategy else {}
    out_dir = Path(args.out_dir) if args.out_dir else None
    if out_dir:
        out_dir.mkdir(parents=True, exist_ok=True)

    found = 0
    with pdfplumber.open(args.file, password=args.password) as pdf:
        for number in parse_pages(args.pages, len(pdf.pages)):
            for index, table in enumerate(pdf.pages[number - 1].extract_tables(settings), start=1):
                rows = [[clean(cell) for cell in row] for row in table if row and any(cell for cell in row)]
                if not rows:
                    continue
                found += 1
                if out_dir:
                    target = out_dir / f"page{number}_table{index}.csv"
                    target.write_text(to_csv(rows), encoding="utf-8-sig")
                    print(f"page {number}, table {index}: {len(rows)} rows -> {target.resolve()}")
                    continue
                print(f"### Page {number}, table {index}\n")
                print(markdown(rows) if args.format == "markdown" else to_csv(rows))
                print()

    if found == 0:
        print("No tables found. Try --text-strategy for tables without ruling lines.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
