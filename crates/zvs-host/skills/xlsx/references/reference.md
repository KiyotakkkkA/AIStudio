# openpyxl notes

```python
from openpyxl import Workbook, load_workbook
from openpyxl.styles import Font, PatternFill, Alignment

book = load_workbook("in.xlsx")
book_values = load_workbook("in.xlsx", data_only=True)
sheet = book["Лист1"]
sheet["A1"].value
sheet.cell(row=2, column=3).value = "=SUM(A2:B2)"
sheet.max_row, sheet.max_column, sheet.dimensions
for row in sheet.iter_rows(min_row=2, values_only=True):
    ...
book.save("out.xlsx")
```

- `data_only=True` returns the values Excel cached at the last save; formulas are not evaluated by openpyxl. Saving a workbook opened with `data_only=True` replaces every formula with its cached value, so never save it.
- A workbook written by openpyxl has no cached values until it is opened and saved in Excel; reading it back with `data_only=True` returns `None` for formula cells.
- `.xlsm` keeps its macros only with `load_workbook(path, keep_vba=True)`.
- openpyxl drops charts and images it cannot read and does not keep pivot tables' caches. Before editing a workbook that has them, tell the user.
- Merged cells: only the top-left cell holds a value.
- Dates are stored as numbers with a date format; openpyxl converts them to `datetime` on read.
- Column letters: `openpyxl.utils.get_column_letter(5) == "E"`, `column_index_from_string("E") == 5`.

## Formula reminders

- English function names, `,` as the argument separator, `.` as the decimal point inside formulas.
- Absolute references with `$`: `=B2*$F$1`.
- Text compares are case-insensitive; wrap lookups in `IFERROR(…, "")` only when an empty result is truly expected, never to hide a broken reference.
- Dynamic-array functions (`FILTER`, `UNIQUE`, `XLOOKUP`) need Excel 365 or 2021; prefer `INDEX`/`MATCH` when the user's Excel version is unknown.
