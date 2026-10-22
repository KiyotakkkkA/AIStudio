# /// script
# requires-python = ">=3.10"
# dependencies = ["reportlab==5.0.1"]
# ///
import argparse
import html
import os
import re
import sys
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.enums import TA_LEFT
from reportlab.lib.pagesizes import A4, A5, LETTER, landscape
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.pdfmetrics import registerFontFamily
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (
    ListFlowable,
    ListItem,
    PageBreak,
    Paragraph,
    Preformatted,
    SimpleDocTemplate,
    Spacer,
    Table,
    TableStyle,
)

PAGE_SIZES = {"A4": A4, "A5": A5, "LETTER": LETTER}

FAMILIES = [
    ("arial.ttf", "arialbd.ttf", "ariali.ttf", "arialbi.ttf"),
    ("DejaVuSans.ttf", "DejaVuSans-Bold.ttf", "DejaVuSans-Oblique.ttf", "DejaVuSans-BoldOblique.ttf"),
    ("LiberationSans-Regular.ttf", "LiberationSans-Bold.ttf", "LiberationSans-Italic.ttf", "LiberationSans-BoldItalic.ttf"),
]
MONO = ["consola.ttf", "cour.ttf", "DejaVuSansMono.ttf", "LiberationMono-Regular.ttf"]


def font_dirs() -> list[Path]:
    dirs = []
    windows = os.environ.get("WINDIR")
    if windows:
        dirs.append(Path(windows) / "Fonts")
    dirs += [
        Path("C:/Windows/Fonts"),
        Path("/usr/share/fonts/truetype/dejavu"),
        Path("/usr/share/fonts/dejavu"),
        Path("/usr/share/fonts/truetype/liberation"),
        Path("/Library/Fonts"),
        Path("/System/Library/Fonts/Supplemental"),
    ]
    return [path for path in dirs if path.is_dir()]


def find(name: str) -> Path | None:
    for folder in font_dirs():
        candidate = folder / name
        if candidate.is_file():
            return candidate
    return None


def register_fonts() -> tuple[str, str]:
    body, mono = "Helvetica", "Courier"
    for family in FAMILIES:
        paths = [find(name) for name in family]
        if paths[0] is None:
            continue
        names = ["Body", "Body-Bold", "Body-Italic", "Body-BoldItalic"]
        for name, path in zip(names, paths):
            pdfmetrics.registerFont(TTFont(name, str(path or paths[0])))
        registerFontFamily("Body", normal="Body", bold="Body-Bold", italic="Body-Italic", boldItalic="Body-BoldItalic")
        body = "Body"
        break
    for name in MONO:
        path = find(name)
        if path is not None:
            pdfmetrics.registerFont(TTFont("Mono", str(path)))
            mono = "Mono"
            break
    return body, mono


def inline(text: str, mono: str) -> str:
    parts = re.split(r"(`[^`]+`)", text)
    out = []
    for part in parts:
        if part.startswith("`") and part.endswith("`") and len(part) > 1:
            out.append(f'<font face="{mono}">{html.escape(part[1:-1])}</font>')
            continue
        escaped = html.escape(part)
        escaped = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", escaped)
        escaped = re.sub(r"__(.+?)__", r"<b>\1</b>", escaped)
        escaped = re.sub(r"(?<![*\w])\*(?!\s)(.+?)(?<!\s)\*(?!\*)", r"<i>\1</i>", escaped)
        escaped = re.sub(r"\[([^\]]+)\]\((https?://[^)]+)\)", r'<link href="\2" color="blue">\1</link>', escaped)
        out.append(escaped)
    return "".join(out)


def styles(body: str, mono: str) -> dict[str, ParagraphStyle]:
    base = getSampleStyleSheet()
    normal = ParagraphStyle("BodyText", parent=base["Normal"], fontName=body, fontSize=11, leading=15, spaceAfter=6, alignment=TA_LEFT)
    return {
        "body": normal,
        "h1": ParagraphStyle("H1", parent=normal, fontName=f"{body}-Bold" if body == "Body" else "Helvetica-Bold", fontSize=20, leading=25, spaceBefore=6, spaceAfter=10),
        "h2": ParagraphStyle("H2", parent=normal, fontName=f"{body}-Bold" if body == "Body" else "Helvetica-Bold", fontSize=15, leading=20, spaceBefore=10, spaceAfter=6),
        "h3": ParagraphStyle("H3", parent=normal, fontName=f"{body}-Bold" if body == "Body" else "Helvetica-Bold", fontSize=12.5, leading=17, spaceBefore=8, spaceAfter=4),
        "cell": ParagraphStyle("Cell", parent=normal, fontSize=9.5, leading=12, spaceAfter=0),
        "code": ParagraphStyle("Code", parent=normal, fontName=mono, fontSize=9, leading=12, backColor=colors.HexColor("#f3f3f3"), borderPadding=4, spaceBefore=4, spaceAfter=8),
    }


def table_of(rows: list[list[str]], sheet: dict[str, ParagraphStyle], mono: str, width: float) -> Table:
    count = max(len(row) for row in rows)
    data = [[Paragraph(inline(cell, mono), sheet["cell"]) for cell in row + [""] * (count - len(row))] for row in rows]
    table = Table(data, colWidths=[width / count] * count, repeatRows=1)
    table.setStyle(
        TableStyle(
            [
                ("GRID", (0, 0), (-1, -1), 0.5, colors.HexColor("#9a9a9a")),
                ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#e8e8e8")),
                ("VALIGN", (0, 0), (-1, -1), "TOP"),
                ("TOPPADDING", (0, 0), (-1, -1), 3),
                ("BOTTOMPADDING", (0, 0), (-1, -1), 3),
            ]
        )
    )
    return table


def split_row(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def build(markdown: str, sheet: dict[str, ParagraphStyle], mono: str, width: float) -> list:
    story: list = []
    lines = markdown.replace("\r\n", "\n").split("\n")
    index = 0
    paragraph: list[str] = []

    def flush() -> None:
        if paragraph:
            story.append(Paragraph(inline(" ".join(paragraph), mono), sheet["body"]))
            paragraph.clear()

    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if stripped.startswith("```"):
            flush()
            block = []
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("```"):
                block.append(lines[index])
                index += 1
            story.append(Preformatted("\n".join(block), sheet["code"]))
            index += 1
            continue
        if stripped in ("---", "***", "___"):
            flush()
            story.append(PageBreak())
            index += 1
            continue
        heading = re.match(r"^(#{1,6})\s+(.*)$", stripped)
        if heading:
            flush()
            level = min(len(heading.group(1)), 3)
            story.append(Paragraph(inline(heading.group(2), mono), sheet[f"h{level}"]))
            index += 1
            continue
        if stripped.startswith("|") and index + 1 < len(lines) and re.match(r"^\|?\s*:?-{2,}", lines[index + 1].strip()):
            flush()
            rows = [split_row(stripped)]
            index += 2
            while index < len(lines) and lines[index].strip().startswith("|"):
                rows.append(split_row(lines[index]))
                index += 1
            story.append(table_of(rows, sheet, mono, width))
            story.append(Spacer(1, 6))
            continue
        bullet = re.match(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$", line)
        if bullet:
            flush()
            ordered = bullet.group(2)[0].isdigit()
            items = []
            while index < len(lines):
                match = re.match(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$", lines[index])
                if not match:
                    break
                items.append(ListItem(Paragraph(inline(match.group(3), mono), sheet["body"])))
                index += 1
            story.append(
                ListFlowable(
                    items,
                    bulletType="1" if ordered else "bullet",
                    start="1" if ordered else "•",
                    bulletFontName=sheet["body"].fontName,
                    leftIndent=14,
                )
            )
            continue
        if not stripped:
            flush()
            index += 1
            continue
        paragraph.append(stripped)
        index += 1
    flush()
    return story


def main() -> int:
    parser = argparse.ArgumentParser(description="Render Markdown into a PDF.")
    parser.add_argument("input", help="a Markdown file, or - for stdin")
    parser.add_argument("output")
    parser.add_argument("--title", default=None)
    parser.add_argument("--author", default=None)
    parser.add_argument("--page-size", choices=sorted(PAGE_SIZES), default="A4")
    parser.add_argument("--landscape", action="store_true")
    parser.add_argument("--margin-mm", type=float, default=20)
    args = parser.parse_args()

    markdown = sys.stdin.read() if args.input == "-" else Path(args.input).read_text(encoding="utf-8-sig")
    body, mono = register_fonts()
    size = PAGE_SIZES[args.page_size]
    if args.landscape:
        size = landscape(size)
    margin = args.margin_mm * mm
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    document = SimpleDocTemplate(
        str(output),
        pagesize=size,
        leftMargin=margin,
        rightMargin=margin,
        topMargin=margin,
        bottomMargin=margin,
        title=args.title or output.stem,
        author=args.author or "",
    )
    sheet = styles(body, mono)
    story = build(markdown, sheet, mono, size[0] - 2 * margin)
    if not story:
        print("The input is empty.", file=sys.stderr)
        return 2
    document.build(story)
    print(f"Wrote {output.resolve()}")
    if body == "Helvetica":
        print("No TrueType font found: Cyrillic text may not render.", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
