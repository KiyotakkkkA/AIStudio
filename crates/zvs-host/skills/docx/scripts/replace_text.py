# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import json
import sys
from pathlib import Path

from docx import Document


def replace_in_paragraph(paragraph, old: str, new: str) -> int:
    runs = paragraph.runs
    if not runs or old not in paragraph.text:
        return 0
    texts = [run.text for run in runs]
    joined = "".join(texts)
    starts = []
    position = 0
    for text in texts:
        starts.append(position)
        position += len(text)
    matches = []
    cursor = joined.find(old)
    while cursor != -1:
        matches.append(cursor)
        cursor = joined.find(old, cursor + len(old))

    def locate(offset: int) -> tuple[int, int]:
        for index in range(len(texts) - 1, -1, -1):
            if starts[index] <= offset and (texts[index] or index == 0):
                return index, offset - starts[index]
        return 0, offset

    for match in reversed(matches):
        first, first_offset = locate(match)
        last, last_offset = locate(match + len(old) - 1)
        if first == last:
            text = texts[first]
            texts[first] = text[:first_offset] + new + text[first_offset + len(old):]
        else:
            texts[first] = texts[first][:first_offset] + new
            for middle in range(first + 1, last):
                texts[middle] = ""
            texts[last] = texts[last][last_offset + 1:]
        starts = []
        position = 0
        for text in texts:
            starts.append(position)
            position += len(text)
    for run, text in zip(runs, texts):
        if run.text != text:
            run.text = text
    return len(matches)


def paragraphs_of(container):
    for paragraph in container.paragraphs:
        yield paragraph
    for table in getattr(container, "tables", []):
        for row in table.rows:
            for cell in row.cells:
                yield from paragraphs_of(cell)


def all_paragraphs(document):
    yield from paragraphs_of(document)
    seen = set()
    for section in document.sections:
        for part in (
            section.header,
            section.footer,
            section.first_page_header,
            section.first_page_footer,
            section.even_page_header,
            section.even_page_footer,
        ):
            if part.is_linked_to_previous or part._element in seen:
                continue
            seen.add(part._element)
            yield from paragraphs_of(part)


def main() -> int:
    parser = argparse.ArgumentParser(description="Replace text in a .docx while keeping formatting.")
    parser.add_argument("file")
    parser.add_argument("values", help='a JSON file of {"old": "new"}, or - for stdin')
    parser.add_argument("output")
    args = parser.parse_args()

    text = sys.stdin.read() if args.values == "-" else Path(args.values).read_text(encoding="utf-8-sig")
    pairs = json.loads(text)
    if not isinstance(pairs, dict) or not pairs:
        print('The values must be a non-empty JSON object of {"old": "new"}.', file=sys.stderr)
        return 2
    if Path(args.output).resolve() == Path(args.file).resolve():
        print("Write to a new file; the input is never overwritten.", file=sys.stderr)
        return 2

    document = Document(args.file)
    counts = {old: 0 for old in pairs}
    visited = set()
    for paragraph in all_paragraphs(document):
        if paragraph._p in visited:
            continue
        visited.add(paragraph._p)
        for old, new in pairs.items():
            if old:
                counts[old] += replace_in_paragraph(paragraph, str(old), "" if new is None else str(new))

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    document.save(output)
    print(json.dumps({"replaced": counts, "output": str(output.resolve())}, ensure_ascii=False, indent=2))
    missing = [old for old, count in counts.items() if count == 0]
    if missing:
        print("Not found: " + ", ".join(missing), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
