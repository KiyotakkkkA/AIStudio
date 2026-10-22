# Workbook spec for write_workbook.py

`write_workbook.py` reads one JSON object. Every key is optional.

```json
{
  "sheets": [
    {
      "name": "Продажи",
      "start": "A1",
      "rows": [
        ["Месяц", "Выручка", "Расходы", "Прибыль"],
        ["2026-01-31", 120000, 80000, "=B2-C2"],
        ["2026-02-28", 135000, 82000, "=B3-C3"]
      ],
      "cells": { "F1": "Итого прибыль", "G1": "=SUM(D2:D3)" },
      "header": true,
      "numberFormats": { "B2:D3": "#,##0 ₽", "G1": "#,##0 ₽" },
      "styles": {
        "F1:G1": { "bold": true, "fill": "FFF2CC", "border": true },
        "B2:C3": { "color": "0000FF" }
      },
      "merge": [],
      "freeze": "A2",
      "autofilter": true,
      "columnWidths": "auto",
      "rowHeights": { "1": 24 },
      "parseNumbers": false
    }
  ],
  "deleteSheets": [],
  "names": { "Ставка": "Параметры!$B$2" },
  "active": "Продажи"
}
```

## Sheets

- `name` — up to 31 characters. With `--base`, an existing sheet of that name is changed in place; otherwise a new sheet is added.
- `rows` — a 2D array written from `start` (default `A1`). `null` leaves a cell untouched.
- `cells` — single cells by address, written after `rows`.
- A string starting with `=` is a formula. Use English function names and `,` between arguments: `=IF(B2>0,B2/C2,0)`, `=VLOOKUP(A2,Справочник!A:B,2,FALSE)`, `=SUMIFS(C:C,A:A,"Москва")`. Quote sheet names with spaces: `='Итоги 2026'!B4`.
- A string `YYYY-MM-DD` or `YYYY-MM-DD HH:MM` becomes a real date with format `DD.MM.YYYY`.
- `parseNumbers: true` turns numeric strings (`"1200"`, `"3,5"`) into numbers; CSV import sets it.
- `header` (default `true`) makes the first row of `rows` bold with a fill and a bottom border, freezes below it and adds an autofilter over the written block. Set it to `false` for sheets without a header row.
- `freeze` — the top-left cell of the scrolling area (`"B2"` freezes the first row and column); `null` removes freezing.
- `autofilter` — `true`, `false` or a range.
- `numberFormats` — range → Excel format code: `"0.00"`, `"#,##0"`, `"0%"`, `"#,##0.00 ₽"`, `"DD.MM.YYYY"`, `"[h]:mm"`.
- `styles` — range → `{ bold, italic, size, font, color, fill, align, valign, wrap, border, numberFormat }`. Colours are hex `RRGGBB`. `align` is `left`, `center`, `right`; `valign` is `top`, `center`, `bottom`.
- `merge` — ranges to merge. Write the value into the top-left cell.
- `columnWidths` — `"auto"` (default) or `{ "A": 20, "B": 12 }`.

## Workbook

- `deleteSheets` — sheet names to remove (the last sheet is never removed).
- `names` — defined names, for inputs referenced from many formulas.
- `active` — the sheet shown when the file opens.

## Editing an existing workbook

Pass `--base in.xlsx` and only the changes:

```json
{ "sheets": [ { "name": "Бюджет", "header": false, "columnWidths": null, "cells": { "C14": 250000, "C15": "=C14*1.2" } } ] }
```

Set `"header": false` and `"columnWidths": null` so the existing formatting is not touched. Charts, images and pivot tables in the base file may be lost when it is saved again; warn the user when `xlsx_info.py` reports any.
