# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2", "reportlab==5.0.1"]
# ///
import argparse
import io
import os
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from reportlab.lib.colors import Color
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

FONT_CANDIDATES = [
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
]


def font_name() -> str:
    windows = os.environ.get("WINDIR")
    candidates = ([f"{windows}/Fonts/arial.ttf"] if windows else []) + FONT_CANDIDATES
    for path in candidates:
        if Path(path).is_file():
            pdfmetrics.registerFont(TTFont("Body", path))
            return "Body"
    return "Helvetica"


def text_stamp(text: str, width: float, height: float, opacity: float, size: float) -> PdfReader:
    buffer = io.BytesIO()
    sheet = canvas.Canvas(buffer, pagesize=(width, height))
    sheet.setFillColor(Color(0.5, 0.5, 0.5, alpha=opacity))
    sheet.setFont(font_name(), size)
    sheet.translate(width / 2, height / 2)
    sheet.rotate(45)
    sheet.drawCentredString(0, -size / 3, text)
    sheet.save()
    buffer.seek(0)
    return PdfReader(buffer)


def main() -> int:
    parser = argparse.ArgumentParser(description="Stamp a watermark on every page.")
    parser.add_argument("file")
    parser.add_argument("output")
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--text")
    source.add_argument("--stamp", help="a one-page PDF to lay over each page")
    parser.add_argument("--opacity", type=float, default=0.18)
    parser.add_argument("--size", type=float, default=60)
    parser.add_argument("--under", action="store_true", help="place the watermark under the page content")
    args = parser.parse_args()

    reader = PdfReader(args.file)
    writer = PdfWriter()
    stamp_page = PdfReader(args.stamp).pages[0] if args.stamp else None
    cache: dict[tuple[float, float], object] = {}
    for page in reader.pages:
        if stamp_page is None:
            size = (float(page.mediabox.width), float(page.mediabox.height))
            if size not in cache:
                cache[size] = text_stamp(args.text, size[0], size[1], args.opacity, args.size).pages[0]
            overlay = cache[size]
        else:
            overlay = stamp_page
        page.merge_page(overlay, over=not args.under)
        writer.add_page(page)

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as handle:
        writer.write(handle)
    print(f"Watermarked {len(reader.pages)} pages -> {output.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
