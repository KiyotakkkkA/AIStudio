# Files: CSV, TSV, Excel, JSON, Parquet, pasted tables

## Find the files

The user names a file or a folder. List a folder with `fs.list` (`glob: "**/*.xlsx"`) and confirm with the user which files belong to the dashboard when several look alike (`sales_v2_final.xlsx`, `sales_final_new.xlsx`).

## Profile first

```
profile_data.py C:/data/sales.csv
profile_data.py C:/data/report.xlsx            prints "sheets" and profiles the first one
profile_data.py "C:/data/report.xlsx#Итоги"    a specific sheet
```

Read from the profile: the row count, each column's type, the share of empty values (`nulls`), `min`/`max` for numbers and dates, and the frequent values (`top`) of text columns. Decide from it:

- which column is the date and at what grain (day, month);
- which columns are measures (numbers) and which are dimensions (text with few distinct values);
- what needs cleaning: numbers typed as `VARCHAR`, dates typed as text, spelling variants in `top`, a total row at the bottom (a `max` far above the rest).

## CSV and TSV

- Delimiter and encoding (UTF-8, Windows-1251) are detected. `;`-separated files are read with a decimal comma.
- Numbers like `1 234,50` with a comma-separated file: pass `--decimal-comma`.
- Dates like `31.10.2026`: pass `--date-format %d.%m.%Y`.
- A file with a title block above the header: clean it with SQL after reading as text, or ask the user for a plain export.

## Excel

- Each sheet is a table with the header in its first non-empty row. Formulas are read as their last calculated values; a workbook saved by a program that does not calculate shows empty values — say so and ask for a file saved in Excel.
- Reports with merged headers, subtotals and several tables on one sheet: read the sheet with the `xlsx` skill (`read_sheet.py`) to see its structure, then extract the exact range into a CSV with `query_data.py` or by writing the values with `fs.apply`.
- Hidden rows and filtered views are still read; check totals.

## JSON and JSONL

An array of objects, one object per line (JSONL), or nested JSON. Flatten nested JSON in SQL: `SELECT order.id, unnest(order.items) AS item FROM data`, then `item.price`.

## Parquet

Read directly; profile shows the stored types.

## Tables pasted into the chat or written in the message

Write them into a CSV with `fs.apply` (when available) next to the other data, or put a few rows inline into the spec as `"rows": [...]`. Keep the user's numbers exactly; ask about any value that looks like a typo instead of correcting it.
