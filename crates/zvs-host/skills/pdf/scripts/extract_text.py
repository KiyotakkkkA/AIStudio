# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber==0.11.10"]
# ///
import argparse
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


def main() -> int:
    parser = argparse.ArgumentParser(description="Extract text from a PDF, page by page.")
    parser.add_argument("file")
    parser.add_argument("--pages", help="1-based pages, e.g. 1-3,7,10-")
    parser.add_argument("--layout", action="store_true", help="keep the visual layout")
    parser.add_argument("--password", default=None)
    parser.add_argument("--output", help="write the text to this file instead of stdout")
    args = parser.parse_args()

    chunks: list[str] = []
    with pdfplumber.open(args.file, password=args.password) as pdf:
        count = len(pdf.pages)
        selected = parse_pages(args.pages, count)
        empty = 0
        for number in selected:
            page = pdf.pages[number - 1]
            text = page.extract_text(layout=args.layout) or ""
            if not text.strip():
                empty += 1
            chunks.append(f"--- page {number} of {count} ---\n{text.rstrip()}\n")
        if selected and empty == len(selected):
            chunks.append("No text layer on the selected pages: the pages are probably scanned images.\n")

    result = "\n".join(chunks)
    if args.output:
        Path(args.output).write_text(result, encoding="utf-8")
        print(f"Wrote {len(result)} characters from {len(selected)} pages to {Path(args.output).resolve()}")
    else:
        sys.stdout.write(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
