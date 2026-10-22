# python-docx notes

```python
from docx import Document
from docx.shared import Pt, Mm
from docx.enum.text import WD_ALIGN_PARAGRAPH

document = Document("in.docx")
for paragraph in document.paragraphs:
    paragraph.style.name, paragraph.text

paragraph = document.add_paragraph("Текст", style="Normal")
paragraph.alignment = WD_ALIGN_PARAGRAPH.JUSTIFY
paragraph.paragraph_format.first_line_indent = Mm(12.5)
run = paragraph.add_run(" жирный")
run.bold = True
run.font.size = Pt(14)

table = document.add_table(rows=2, cols=3)
table.style = "Table Grid"
table.cell(0, 0).text = "Заголовок"

section = document.sections[0]
section.left_margin = Mm(30)
document.save("out.docx")
```

- `document.paragraphs` skips paragraphs inside tables; walk `document.tables` for those, and `section.header` / `section.footer` for headers and footers.
- Setting `paragraph.text` or `run.text` drops the formatting of the runs it replaces. `replace_text.py` keeps it by editing runs in place.
- Font names for Cyrillic must be set on all four slots of `w:rFonts` (`ascii`, `hAnsi`, `cs`, `eastAsia`), otherwise Word may fall back to another font. `markdown_to_docx.py --font` does this.
- Built-in style names are English (`Heading 1`, `List Bullet`, `Table Grid`) even in a Russian Word; a template may lack some of them.
- Images: `document.add_picture(path, width=Mm(120))`.
- Page numbers, tables of contents and cross-references are fields. python-docx cannot compute them; Word updates them when the document is opened (or with F9).
