# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter


def parse_ranges(spec: str, count: int) -> list[tuple[int, int]]:
    ranges: list[tuple[int, int]] = []
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, _, end = part.partition("-")
            first = int(start) if start else 1
            last = int(end) if end else count
        else:
            first = last = int(part)
        first, last = max(first, 1), min(last, count)
        if first <= last:
            ranges.append((first, last))
    return ranges


def main() -> int:
    parser = argparse.ArgumentParser(description="Split a PDF into several files.")
    parser.add_argument("file")
    parser.add_argument("out_dir")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--ranges", help="1-based ranges, one output file each, e.g. 1-3,4-10")
    group.add_argument("--each", action="store_true", help="one file per page")
    parser.add_argument("--prefix", default=None)
    args = parser.parse_args()

    reader = PdfReader(args.file)
    count = len(reader.pages)
    ranges = [(page, page) for page in range(1, count + 1)] if args.each else parse_ranges(args.ranges, count)
    if not ranges:
        print(f"No pages selected; the document has {count} pages.", file=sys.stderr)
        return 2

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    prefix = args.prefix or Path(args.file).stem
    for first, last in ranges:
        writer = PdfWriter()
        for index in range(first - 1, last):
            writer.add_page(reader.pages[index])
        name = f"{prefix}_p{first}.pdf" if first == last else f"{prefix}_p{first}-{last}.pdf"
        target = out_dir / name
        with target.open("wb") as handle:
            writer.write(handle)
        print(f"pages {first}-{last} -> {target.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
