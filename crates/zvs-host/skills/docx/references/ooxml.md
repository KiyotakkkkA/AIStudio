# Editing the XML of a .docx

A `.docx` is a ZIP package. The text lives in `word/document.xml`; headers and footers in `word/header*.xml` and `word/footer*.xml`; comments in `word/comments.xml`; styles in `word/styles.xml`; numbering in `word/numbering.xml`; relationships in `word/_rels/document.xml.rels`.

## Workflow without unpacking

1. Find the place: `ooxml.py show <file> word/document.xml --find "уникальный текст"`. It prints the matching lines of pretty-printed XML with their neighbours.
2. Replace an exact fragment: `ooxml.py replace <file> <out.docx> --part word/document.xml` with stdin `{"<old fragment>": "<new fragment>"}`.
3. Copy `old` from a single line of `show` output, keep it short and unique, and check the counts it prints. The result is refused when it is not well-formed XML.
4. Check the result with `docx_to_markdown.py` or `docx_info.py`.

`unpack`/`pack` exist for bulk work with another script; they keep the package byte-compatible apart from whitespace between tags.

## The shapes that matter

A paragraph with two runs:

```xml
<w:p>
  <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
  <w:r><w:rPr><w:b/></w:rPr><w:t>Жирный</w:t></w:r>
  <w:r><w:t xml:space="preserve"> обычный</w:t></w:r>
</w:p>
```

- Text is split into runs (`w:r`) wherever formatting changes, and Word often splits it for no visible reason. Search for short fragments.
- `xml:space="preserve"` is required on `w:t` that starts or ends with a space.
- Page break: `<w:r><w:br w:type="page"/></w:r>`.

Tracked changes:

```xml
<w:ins w:id="101" w:author="ZVS" w:date="2026-01-01T00:00:00Z"><w:r><w:t>новый текст</w:t></w:r></w:ins>
<w:del w:id="102" w:author="ZVS" w:date="2026-01-01T00:00:00Z"><w:r><w:delText>старый текст</w:delText></w:r></w:del>
```

- Inside `w:del`, text is `w:delText`, not `w:t`.
- `w:id` must be unique across the document.

A comment needs three pieces: `<w:commentRangeStart w:id="5"/>` and `<w:commentRangeEnd w:id="5"/>` around the text, a run `<w:r><w:commentReference w:id="5"/></w:r>` after the end, and the comment body with the same id in `word/comments.xml`. When the document has no `comments.xml`, adding one also needs a relationship and a content type; explain this to the user rather than attempting it.
