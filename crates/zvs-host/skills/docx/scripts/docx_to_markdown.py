# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import sys

from docx import Document
from docx.table import Table
from docx.text.paragraph import Paragraph

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
    for run in paragraph.runs:
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
    parser.add_argument("--with-index", action="store_true", help="prefix every block with its index")
    args = parser.parse_args()

    document = Document(args.file)
    formats = numbering_formats(document)
    shift = 1 if any(p.style is not None and p.style.name == "Title" for p in document.paragraphs) else 0
    blocks = []
    for child in document.element.body.iterchildren():
        if child.tag == f"{W}p":
            blocks.append(Paragraph(child, document))
        elif child.tag == f"{W}tbl":
            blocks.append(Table(child, document))

    end = min(len(blocks), args.start + max(args.limit, 1))
    out = []
    for index in range(max(args.start, 0), end):
        block = blocks[index]
        text = table_markdown(block) if isinstance(block, Table) else paragraph_markdown(block, formats, shift)
        if not text:
            continue
        out.append(f"[{index}] {text}" if args.with_index else text)
    sys.stdout.write("\n\n".join(out) + "\n")
    if end < len(blocks):
        print(f"\n[blocks {args.start}-{end - 1} of {len(blocks)}; continue with --start {end}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())
