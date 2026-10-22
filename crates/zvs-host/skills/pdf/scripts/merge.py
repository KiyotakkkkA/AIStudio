# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import sys
from pathlib import Path

from pypdf import PdfWriter


def main() -> int:
    parser = argparse.ArgumentParser(description="Merge PDF files in the given order.")
    parser.add_argument("output")
    parser.add_argument("inputs", nargs="+")
    parser.add_argument("--bookmarks", action="store_true", help="add one bookmark per input file")
    args = parser.parse_args()

    output = Path(args.output)
    if any(Path(item).resolve() == output.resolve() for item in args.inputs):
        print("The output must not be one of the inputs.", file=sys.stderr)
        return 2

    writer = PdfWriter()
    for item in args.inputs:
        start = len(writer.pages)
        writer.append(item)
        if args.bookmarks:
            writer.add_outline_item(Path(item).stem, start)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as handle:
        writer.write(handle)
    print(f"Merged {len(args.inputs)} files, {len(writer.pages)} pages -> {output.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
