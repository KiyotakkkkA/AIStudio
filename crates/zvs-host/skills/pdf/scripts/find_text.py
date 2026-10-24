# /// script
# requires-python = ">=3.10"
# dependencies = ["pdfplumber==0.11.10"]
# ///
import argparse
import json
import re
import sys

import pdfplumber


def main() -> int:
    parser = argparse.ArgumentParser(description="Find the pages of a PDF that contain a text, with the surrounding words.")
    parser.add_argument("file")
    parser.add_argument("query", nargs="+", help="one or more texts; each is searched separately")
    parser.add_argument("--case-sensitive", action="store_true")
    parser.add_argument("--context", type=int, default=60, help="characters shown around a match")
    parser.add_argument("--password", default=None)
    args = parser.parse_args()

    flags = 0 if args.case_sensitive else re.IGNORECASE
    patterns = {query: re.compile(r"\s+".join(map(re.escape, query.split())), flags) for query in args.query}
    found: dict[str, list[dict]] = {query: [] for query in args.query}
    with pdfplumber.open(args.file, password=args.password) as pdf:
        count = len(pdf.pages)
        empty = 0
        for number, page in enumerate(pdf.pages, start=1):
            text = page.extract_text() or ""
            if not text.strip():
                empty += 1
                continue
            for query, pattern in patterns.items():
                for match in pattern.finditer(text):
                    start, end = max(match.start() - args.context, 0), min(match.end() + args.context, len(text))
                    found[query].append({"page": number, "snippet": " ".join(text[start:end].split())})
    result = {
        "pages": count,
        "matches": {query: {"pages": sorted({hit["page"] for hit in hits}), "hits": hits[:30]} for query, hits in found.items()},
    }
    if empty == count:
        result["note"] = "No text layer: the pages are probably scanned images."
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
