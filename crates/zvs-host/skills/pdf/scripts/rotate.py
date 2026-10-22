# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter


def parse_pages(spec: str | None, count: int) -> set[int]:
    if not spec:
        return set(range(1, count + 1))
    pages: set[int] = set()
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, _, end = part.partition("-")
            pages.update(range(int(start) if start else 1, (int(end) if end else count) + 1))
        else:
            pages.add(int(part))
    return {page for page in pages if 1 <= page <= count}


def main() -> int:
    parser = argparse.ArgumentParser(description="Rotate PDF pages clockwise.")
    parser.add_argument("file")
    parser.add_argument("output")
    parser.add_argument("--angle", type=int, choices=[90, 180, 270], required=True)
    parser.add_argument("--pages", help="1-based pages; all pages when omitted")
    args = parser.parse_args()

    reader = PdfReader(args.file)
    selected = parse_pages(args.pages, len(reader.pages))
    writer = PdfWriter()
    for number, page in enumerate(reader.pages, start=1):
        if number in selected:
            page.rotate(args.angle)
        writer.add_page(page)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as handle:
        writer.write(handle)
    print(f"Rotated {len(selected)} pages by {args.angle} degrees -> {output.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
