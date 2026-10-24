# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import sys

from docx import Document
from docx.table import Table
from docx.text.paragraph import Paragraph

from _blocks import body_blocks, page_numbers

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
A = "{http://schemas.openxmlformats.org/drawingml/2006/main}"


def numbering_formats(document) -> dict[tuple[str, str], str]:
    try:
        root = document.part.numbering_part.element
    except (KeyError, NotImplementedError, AttributeError):
        return {}
    abstract: dict[str, dict[str, str]] = {}
    for definition in root.findall(f"{W}abstractNum"):
        levels = {}
        for level in definition.findall(f"{W}lvl"):
            fmt = level.find(f"{W}numFmt")
            levels[level.get(f"{W}ilvl")] = fmt.get(f"{W}val") if fmt is not None else "bullet"
        abstract[definition.get(f"{W}abstractNumId")] = levels
    formats: dict[tuple[str, str], str] = {}
    for num in root.findall(f"{W}num"):
        reference = num.find(f"{W}abstractNumId")
        if reference is None:
            continue
        for level, fmt in abstract.get(reference.get(f"{W}val"), {}).items():
            formats[(num.get(f"{W}numId"), level)] = fmt
    return formats


def run_text(paragraph: Paragraph) -> str:
    parts = []
    runs = []
    for item in paragraph.iter_inner_content():
        runs.extend(item.runs if hasattr(item, "runs") else [item])
    for run in runs:
        if run.element.findall(f".//{A}blip"):
            parts.append("![image]")
        text = run.text
        stripped = text.strip()
        if not stripped:
            parts.append(text)
            continue
        mark = "***" if run.bold and run.italic else "**" if run.bold else "*" if run.italic else ""
        parts.append(text.replace(stripped, f"{mark}{stripped}{mark}", 1) if mark else text)
    return "".join(parts)


def list_marker(paragraph: Paragraph, formats: dict[tuple[str, str], str]) -> tuple[int, str] | None:
    properties = paragraph._p.pPr
    numbering = properties.numPr if properties is not None else None
    style = paragraph.style.name.lower() if paragraph.style is not None else ""
    if numbering is not None and numbering.numId is not None:
        level = numbering.ilvl.val if numbering.ilvl is not None else 0
        fmt = formats.get((str(numbering.numId.val), str(level)), "bullet")
        return int(level), "-" if fmt in ("bullet", "none") else "1."
    if "list bullet" in style:
        return 0, "-"
    if "list number" in style:
        return 0, "1."
    return None


def paragraph_markdown(paragraph: Paragraph, formats: dict[tuple[str, str], str], shift: int) -> str:
    text = run_text(paragraph).strip()
    if not text:
        return ""
    style = paragraph.style.name if paragraph.style is not None else ""
    if style == "Title":
        return f"# {text}"
    if style.startswith("Heading"):
        level = style.replace("Heading", "").strip()
        depth = int(level) if level.isdigit() else 1
        return f"{'#' * min(depth + shift, 6)} {text}"
    marker = list_marker(paragraph, formats)
    if marker:
        level, symbol = marker
        return f"{'  ' * level}{symbol} {text}"
    return text


def table_markdown(table: Table) -> str:
    rows = []
    for row in table.rows:
        cells = []
        previous = None
        for cell in row.cells:
            if previous is not None and cell._tc is previous:
                cells.append("")
                continue
            previous = cell._tc
            cells.append(" ".join(cell.text.split()).replace("|", "\\|"))
        rows.append(cells)
    if not rows:
        return ""
    width = max(len(row) for row in rows)
    rows = [row + [""] * (width - len(row)) for row in rows]
    lines = ["| " + " | ".join(rows[0]) + " |", "|" + " --- |" * width]
    lines += ["| " + " | ".join(row) + " |" for row in rows[1:]]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Print a .docx as Markdown, block by block.")
    parser.add_argument("file")
    parser.add_argument("--start", type=int, default=0, help="index of the first block")
    parser.add_argument("--limit", type=int, default=300, help="how many blocks to print")
    parser.add_argument("--with-index", action="store_true", help="prefix every block with its index and mark page starts")
    args = parser.parse_args()

    document = Document(args.file)
    formats = numbering_formats(document)
    shift = 1 if any(p.style is not None and p.style.name == "Title" for p in document.paragraphs) else 0
    elements = body_blocks(document.element.body)
    pages, page_source = page_numbers(document)

    def render(element) -> str:
        if element.tag == f"{W}tbl":
            return table_markdown(Table(element, document))
        if element.tag == f"{W}p":
            return paragraph_markdown(Paragraph(element, document), formats, shift)
        parts = [paragraph_markdown(Paragraph(node, document), formats, shift) for node in element.iter(f"{W}p")]
        return "\n\n".join(part for part in parts if part)

    start = max(args.start, 0)
    end = min(len(elements), start + max(args.limit, 1))
    out = []
    if args.with_index:
        out.append(f"[pages counted from {page_source}; blocks are 0-based, pages 1-based]")
    for index in range(start, end):
        if args.with_index and (index == start or pages[index] != pages[index - 1]):
            out.append(f"--- page {pages[index]} ---")
        text = render(elements[index])
        if not text:
            if args.with_index:
                out.append(f"[{index}] (empty)")
            continue
        out.append(f"[{index}] {text}" if args.with_index else text)
    sys.stdout.write("\n\n".join(out) + "\n")
    if end < len(elements):
        print(f"\n[blocks {start}-{end - 1} of {len(elements)}; continue with --start {end}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())
