# PDF library notes

The scripts use three libraries. Each script declares its pinned dependencies in the `# /// script` block at the top, and `uv` installs them into the app's isolated cache on first run.

## pypdf — structure

```python
from pypdf import PdfReader, PdfWriter

reader = PdfReader("in.pdf")
len(reader.pages)
reader.metadata
reader.get_fields()

writer = PdfWriter(clone_from=reader)
writer.add_metadata({"/Title": "Отчёт"})
writer.pages[0].rotate(90)
writer.write("out.pdf")
```

- Page objects can be moved between readers and writers: `writer.add_page(reader.pages[3])`.
- `writer.append(path, pages=(0, 5))` copies a page range with its bookmarks.
- `page.merge_page(other, over=True)` overlays one page on another; that is how stamps and watermarks work.
- Encrypted files need `reader.decrypt(password)` before anything else. AES needs the `pypdf[crypto]` extra.

## pdfplumber — content

```python
import pdfplumber

with pdfplumber.open("in.pdf") as pdf:
    page = pdf.pages[0]
    page.extract_text()
    page.extract_text(layout=True)
    page.extract_tables()
    page.extract_words()
    page.crop((x0, top, x1, bottom)).extract_text()
```

- Coordinates are PDF points (1/72 inch) with `top` measured from the top of the page.
- `extract_tables({"vertical_strategy": "text", "horizontal_strategy": "text"})` finds tables drawn without lines.
- `page.chars` gives every character with its font and size, useful for finding headings by font size.

## reportlab — new documents

- `platypus` (`SimpleDocTemplate`, `Paragraph`, `Table`, `Spacer`, `PageBreak`) lays out flowing content; `create_pdf.py` uses it.
- `canvas.Canvas` draws at absolute positions; `watermark.py` uses it.
- The built-in fonts (Helvetica, Times, Courier) have no Cyrillic. Register a TrueType font first: `pdfmetrics.registerFont(TTFont("Body", path))`.
- Paragraph text is a small HTML subset: `<b>`, `<i>`, `<u>`, `<font face size color>`, `<br/>`, `<link href>`. Escape `&`, `<`, `>` in user text.

## Scanned documents

A page without a text layer is an image. Text extraction returns nothing, and the scripts cannot read it. Say so to the user; recognising text needs OCR, which is not part of this skill.
