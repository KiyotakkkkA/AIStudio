# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import json
import sys
import zipfile
from collections import Counter

from docx import Document
from docx.shared import Length

from _blocks import BLOCK_TAGS, page_numbers

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def mm(value: Length | None) -> float | None:
    return None if value is None else round(value.mm, 1)


def main() -> int:
    parser = argparse.ArgumentParser(description="Print the structure of a .docx file as JSON.")
    parser.add_argument("file")
    args = parser.parse_args()

    document = Document(args.file)
    body = document.element.body
    paragraphs = document.paragraphs
    styles = Counter(paragraph.style.name for paragraph in paragraphs if paragraph.text.strip())
    headings = [
        {"level": paragraph.style.name.replace("Heading", "").strip() or "title", "text": paragraph.text.strip()[:120]}
        for paragraph in paragraphs
        if paragraph.style.name.startswith("Heading") or paragraph.style.name == "Title"
    ]
    words = sum(len(paragraph.text.split()) for paragraph in paragraphs)
    for table in document.tables:
        for row in table.rows:
            for cell in row.cells:
                words += len(cell.text.split())

    with zipfile.ZipFile(args.file) as archive:
        names = archive.namelist()
        document_xml = archive.read("word/document.xml").decode("utf-8", errors="replace")
        comments = archive.read("word/comments.xml").decode("utf-8", errors="replace").count("<w:comment ") if "word/comments.xml" in names else 0

    sections = [
        {
            "orientation": "landscape" if section.orientation == 1 else "portrait",
            "pageWidthMm": mm(section.page_width),
            "pageHeightMm": mm(section.page_height),
            "marginsMm": {
                "top": mm(section.top_margin),
                "right": mm(section.right_margin),
                "bottom": mm(section.bottom_margin),
                "left": mm(section.left_margin),
            },
            "header": " ".join(p.text for p in section.header.paragraphs).strip()[:120] or None,
            "footer": " ".join(p.text for p in section.footer.paragraphs).strip()[:120] or None,
        }
        for section in document.sections
    ]
    pages, page_source = page_numbers(document)
    normal = document.styles["Normal"].font
    info = {
        "blocks": sum(1 for child in body.iterchildren() if child.tag in BLOCK_TAGS),
        "paragraphs": len(paragraphs),
        "words": words,
        "tables": [{"rows": len(table.rows), "columns": len(table.columns)} for table in document.tables],
        "pages": max(pages, default=0),
        "pagesSource": page_source,
        "images": document_xml.count("<a:blip ") + document_xml.count("<v:imagedata "),
        "comments": comments,
        "trackedChanges": document_xml.count("<w:ins ") + document_xml.count("<w:del "),
        "fields": document_xml.count("<w:fldChar ") // 2 + document_xml.count("<w:fldSimple "),
        "baseFont": {"name": normal.name, "sizePt": normal.size.pt if normal.size else None},
        "stylesInUse": dict(styles.most_common(15)),
        "headings": headings[:60],
        "sections": sections,
        "core": {
            "title": document.core_properties.title or None,
            "author": document.core_properties.author or None,
            "modified": str(document.core_properties.modified) if document.core_properties.modified else None,
        },
    }
    print(json.dumps(info, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
