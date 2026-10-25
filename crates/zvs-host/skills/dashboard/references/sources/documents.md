# PDF and Word documents

Numbers in documents come as tables or as figures inside text. Extract them with the bundled `pdf` and `docx` skills, save them as CSV, and continue as with files.

## Tables in a PDF

```
shell.run  argv: ["skill:pdf/scripts/extract_tables.py", "C:/docs/report.pdf", "--pages", "3-5", "--format", "csv", "--out-dir", "C:/Reports/data/report_tables"]
```

- Each table becomes one CSV file in the folder. Add `--text-strategy` for tables drawn without ruling lines.
- Find the pages first with the `pdf` skill's `find_text.py` (a table title, a column name) instead of extracting the whole document.
- Profile every extracted CSV: tables split across pages repeat their header, multi-line cells break rows, and thousands separators come as spaces. Fix these in SQL.
- Scanned PDFs have no text layer; say so, and ask for the original file or a spreadsheet.

## Figures inside PDF text

Extract the text of the relevant pages with `extract_text.py --pages …`, read the numbers, and write them into a small CSV or inline `rows`, one row per figure with its period and unit. Keep a `source` column with the page number, and mention the document in `sources`.

## Word documents

```
shell.run  argv: ["skill:docx/scripts/docx_to_markdown.py", "C:/docs/report.docx", "--with-index"]
```

Tables come out as Markdown tables; copy the needed ones into a CSV with `fs.apply` (keep headers, convert `1 234,5` to `1234.5`). For a long document, page through with `--start` and `--limit`.

## Several documents of the same form

Monthly reports with the same table: extract the table from each, add a column with the period taken from the file name or the title, and union them in SQL:

```sql
SELECT '2026-01-01'::DATE AS month, * FROM jan
UNION ALL BY NAME SELECT '2026-02-01'::DATE AS month, * FROM feb
```

Check that each document's total matches the total row printed in it.
