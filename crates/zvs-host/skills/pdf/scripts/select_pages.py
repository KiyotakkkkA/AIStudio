# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import json
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter


def parse_pages(spec: str, count: int) -> list[int]:
    pages: list[int] = []
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, _, end = part.partition("-")
            first = int(start) if start else 1
            last = int(end) if end else count
            step = 1 if first <= last else -1
            pages.extend(range(first, last + step, step))
        else:
            pages.append(int(part))
    return [page for page in pages if 1 <= page <= count]


def describe(pages: list[int]) -> str:
    parts: list[str] = []
    start = previous = pages[0]
    for page in pages[1:] + [None]:
        if page is not None and page == previous + 1:
            previous = page
            continue
        parts.append(str(start) if start == previous else f"{start}-{previous}")
        if page is not None:
            start = previous = page
    return ",".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(description="Write the chosen pages of a PDF into one new PDF, in the given order, unchanged.")
    parser.add_argument("file")
    parser.add_argument("output")
    parser.add_argument("--pages", required=True, help="1-based pages in output order, e.g. 1 or 3-5,1 or 10-1")
    parser.add_argument("--drop", action="store_true", help="remove the listed pages and keep the rest in their order")
    parser.add_argument("--password", default=None)
    args = parser.parse_args()

    source, target = Path(args.file), Path(args.output)
    if target.resolve() == source.resolve():
        print("Write to a new file; the input is never overwritten.", file=sys.stderr)
        return 2
    reader = PdfReader(str(source))
    if reader.is_encrypted and (args.password is None or not reader.decrypt(args.password)):
        print("The file is encrypted; pass --password.", file=sys.stderr)
        return 2
    count = len(reader.pages)
    listed = parse_pages(args.pages, count)
    dropped = set(listed)
    pages = [page for page in range(1, count + 1) if page not in dropped] if args.drop else listed
    if not pages:
        print(f"No pages selected; the document has {count} pages.", file=sys.stderr)
        return 2

    writer = PdfWriter()
    for page in pages:
        writer.add_page(reader.pages[page - 1])
    if reader.metadata:
        writer.add_metadata({key: value for key, value in reader.metadata.items() if isinstance(value, str)})
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("wb") as handle:
        writer.write(handle)
    print(json.dumps({"output": str(target.resolve()), "pages": describe(pages), "pageCount": len(pages), "sourcePages": count}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
