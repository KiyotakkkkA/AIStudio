# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import re
import sys
from pathlib import Path

from docx import Document
from docx.enum.text import WD_BREAK
from docx.opc.constants import RELATIONSHIP_TYPE
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.shared import Mm, Pt, RGBColor

INLINE = re.compile(r"(\*\*\*.+?\*\*\*|\*\*.+?\*\*|__.+?__|(?<![*\w])\*(?!\s).+?(?<!\s)\*(?!\*)|`[^`]+`|\[[^\]]+\]\([^)]+\))")


def set_font(target, name: str) -> None:
    target.font.name = name
    properties = target.element.get_or_add_rPr()
    fonts = properties.find(qn("w:rFonts"))
    if fonts is None:
        fonts = OxmlElement("w:rFonts")
        properties.insert(0, fonts)
    for attribute in ("w:ascii", "w:hAnsi", "w:cs", "w:eastAsia"):
        fonts.set(qn(attribute), name)


def add_hyperlink(paragraph, text: str, url: str) -> None:
    relation = paragraph.part.relate_to(url, RELATIONSHIP_TYPE.HYPERLINK, is_external=True)
    link = OxmlElement("w:hyperlink")
    link.set(qn("r:id"), relation)
    run = OxmlElement("w:r")
    properties = OxmlElement("w:rPr")
    color = OxmlElement("w:color")
    color.set(qn("w:val"), "0563C1")
    underline = OxmlElement("w:u")
    underline.set(qn("w:val"), "single")
    properties.append(color)
    properties.append(underline)
    run.append(properties)
    node = OxmlElement("w:t")
    node.text = text
    node.set(qn("xml:space"), "preserve")
    run.append(node)
    link.append(run)
    paragraph._p.append(link)


def add_inline(paragraph, text: str, mono: str) -> None:
    for part in INLINE.split(text):
        if not part:
            continue
        if part.startswith("***") and part.endswith("***") and len(part) > 6:
            run = paragraph.add_run(part[3:-3])
            run.bold = run.italic = True
        elif (part.startswith("**") and part.endswith("**") or part.startswith("__") and part.endswith("__")) and len(part) > 4:
            paragraph.add_run(part[2:-2]).bold = True
        elif part.startswith("*") and part.endswith("*") and len(part) > 2:
            paragraph.add_run(part[1:-1]).italic = True
        elif part.startswith("`") and part.endswith("`") and len(part) > 2:
            run = paragraph.add_run(part[1:-1])
            run.font.name = mono
        elif part.startswith("[") and "](" in part:
            label, _, url = part[1:-1].partition("](")
            add_hyperlink(paragraph, label, url)
        else:
            paragraph.add_run(part)


def has_style(document, name: str) -> bool:
    try:
        document.styles[name]
        return True
    except KeyError:
        return False


def clear_body(document) -> None:
    body = document.element.body
    for child in list(body.iterchildren()):
        if child.tag != qn("w:sectPr"):
            body.remove(child)


def split_row(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def add_table(document, rows: list[list[str]], mono: str) -> None:
    width = max(len(row) for row in rows)
    table = document.add_table(rows=len(rows), cols=width)
    if has_style(document, "Table Grid"):
        table.style = "Table Grid"
    for r, row in enumerate(rows):
        for c in range(width):
            cell = table.cell(r, c)
            paragraph = cell.paragraphs[0]
            add_inline(paragraph, row[c] if c < len(row) else "", mono)
            if r == 0:
                for run in paragraph.runs:
                    run.bold = True
    header = table.rows[0]._tr.get_or_add_trPr()
    repeat = OxmlElement("w:tblHeader")
    repeat.set(qn("w:val"), "true")
    header.append(repeat)


def list_style(document, ordered: bool, level: int) -> str | None:
    base = "List Number" if ordered else "List Bullet"
    name = base if level == 0 else f"{base} {min(level + 1, 3)}"
    if has_style(document, name):
        return name
    return base if has_style(document, base) else None


def build(document, markdown: str, mono: str) -> None:
    lines = markdown.replace("\r\n", "\n").split("\n")
    index = 0
    buffer: list[str] = []

    def flush() -> None:
        if buffer:
            add_inline(document.add_paragraph(), " ".join(buffer), mono)
            buffer.clear()

    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if stripped.startswith("```"):
            flush()
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("```"):
                paragraph = document.add_paragraph()
                paragraph.paragraph_format.space_after = Pt(0)
                run = paragraph.add_run(lines[index])
                run.font.name = mono
                run.font.size = Pt(9.5)
                index += 1
            index += 1
            continue
        if stripped in ("---", "***", "___"):
            flush()
            document.add_paragraph().add_run().add_break(WD_BREAK.PAGE)
            index += 1
            continue
        heading = re.match(r"^(#{1,6})\s+(.*)$", stripped)
        if heading:
            flush()
            paragraph = document.add_heading(level=len(heading.group(1)))
            add_inline(paragraph, heading.group(2), mono)
            index += 1
            continue
        if stripped.startswith("|") and index + 1 < len(lines) and re.match(r"^\|?\s*:?-{2,}", lines[index + 1].strip()):
            flush()
            rows = [split_row(stripped)]
            index += 2
            while index < len(lines) and lines[index].strip().startswith("|"):
                rows.append(split_row(lines[index]))
                index += 1
            add_table(document, rows, mono)
            document.add_paragraph()
            continue
        item = re.match(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$", line)
        if item:
            flush()
            level = len(item.group(1).replace("\t", "  ")) // 2
            ordered = item.group(2)[0].isdigit()
            style = list_style(document, ordered, level)
            paragraph = document.add_paragraph(style=style) if style else document.add_paragraph()
            if style is None:
                paragraph.add_run(("1. " if ordered else "• "))
                paragraph.paragraph_format.left_indent = Mm(6 * (level + 1))
            add_inline(paragraph, item.group(3), mono)
            index += 1
            continue
        if not stripped:
            flush()
            index += 1
            continue
        buffer.append(stripped)
        index += 1
    flush()


def main() -> int:
    parser = argparse.ArgumentParser(description="Render Markdown into a .docx document.")
    parser.add_argument("input", help="a Markdown file, or - for stdin")
    parser.add_argument("output")
    parser.add_argument("--template", help="a .docx whose styles and page setup are kept")
    parser.add_argument("--font", default=None)
    parser.add_argument("--size", type=float, default=None, help="base size in points")
    parser.add_argument("--line-spacing", type=float, default=None)
    parser.add_argument("--margins-mm", default=None, help="top,right,bottom,left")
    parser.add_argument("--first-line-indent-mm", type=float, default=None)
    parser.add_argument("--mono-font", default="Consolas")
    parser.add_argument("--title", default=None)
    args = parser.parse_args()

    markdown = sys.stdin.read() if args.input == "-" else Path(args.input).read_text(encoding="utf-8-sig")
    document = Document(args.template) if args.template else Document()
    if args.template:
        clear_body(document)

    normal = document.styles["Normal"]
    if args.font:
        set_font(normal, args.font)
        for level in range(1, 7):
            name = f"Heading {level}"
            if has_style(document, name):
                set_font(document.styles[name], args.font)
                document.styles[name].font.color.rgb = RGBColor(0, 0, 0)
        if has_style(document, "Title"):
            set_font(document.styles["Title"], args.font)
    if args.size:
        normal.font.size = Pt(args.size)
    if args.line_spacing:
        normal.paragraph_format.line_spacing = args.line_spacing
    if args.first_line_indent_mm is not None:
        normal.paragraph_format.first_line_indent = Mm(args.first_line_indent_mm)
    if args.margins_mm:
        top, right, bottom, left = (float(value) for value in args.margins_mm.split(","))
        for section in document.sections:
            section.top_margin, section.right_margin = Mm(top), Mm(right)
            section.bottom_margin, section.left_margin = Mm(bottom), Mm(left)
    if args.title:
        document.core_properties.title = args.title

    build(document, markdown, args.mono_font)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    document.save(output)
    print(f"Wrote {output.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
