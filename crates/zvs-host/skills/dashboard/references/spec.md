# Dashboard spec

The spec is one JSON object. Pass it to `build_dashboard.py` on standard input (`-` as the spec argument) or as a file. The builder runs the dataset queries, checks every block against the columns and writes the page. Relative dataset paths resolve from the spec's folder, or from the run folder when the spec comes on standard input; use absolute paths.

```json
{
  "title": "Продажи, 2026",
  "subtitle": "Выручка по регионам и продуктам",
  "period": "Январь — октябрь 2026",
  "lang": "ru",
  "theme": "auto",
  "sources": ["CRM, выгрузка от 24.10.2026"],
  "notes": ["Выручка без НДС. **Октябрь** — неполный месяц."],
  "datasets": {
    "sales": { "path": "C:/Reports/data/sales_by_month.csv" }
  },
  "filters": [
    { "type": "dateRange", "field": "month", "label": "Период" },
    { "type": "select", "field": "region", "label": "Регион" }
  ],
  "sections": [
    {
      "title": "Итоги",
      "description": "Последний полный месяц",
      "columns": 4,
      "items": [
        { "type": "kpi", "label": "Выручка за месяц", "dataset": "sales", "value": { "field": "revenue", "period": "last", "by": "month" } }
      ]
    }
  ]
}
```

## Top level

| Key        | Required | Meaning                                                                   |
| ---------- | -------- | ------------------------------------------------------------------------- |
| `title`    | yes      | Page title and browser tab name.                                          |
| `subtitle` |          | One line under the title: what the dashboard covers.                      |
| `period`   |          | The data period, shown in the header line.                                |
| `lang`     |          | `ru` (default) or `en`: the language of the page's own controls and number formats. |
| `theme`    |          | `auto` (default, follows the system), `light` or `dark`. The reader can switch it. |
| `sources`  |          | Strings shown in the footer: where the data came from.                    |
| `notes`    |          | Footer notes; `**bold**` is supported.                                    |
| `datasets` | yes      | Named data tables, see below.                                             |
| `filters`  |          | The filter row above all sections.                                        |
| `sections` | yes      | The page content, in order.                                               |

The build time is stamped automatically as `generatedAt`.

## Datasets

```json
"datasets": {
  "monthly": {
    "sources": { "sales": "C:/data/sales.xlsx#2026" },
    "sql": "SELECT date_trunc('month', \"Дата\")::DATE AS month, \"Регион\" AS region, sum(\"Сумма\") AS revenue FROM sales GROUP BY ALL"
  },
  "sales":  { "path": "C:/data/sales.csv" },
  "plan":   { "path": "C:/data/plan.xlsx", "sheet": "2026" },
  "stages": { "rows": [ { "stage": "Визит", "users": 48210 }, { "stage": "Оплата", "users": 1180 } ] }
}
```

| Key         | Meaning                                                                                  |
| ----------- | ---------------------------------------------------------------------------------------- |
| `sql`       | **Preferred.** One DuckDB `SELECT` run while building. Needs `sources`. Name output columns in snake_case. |
| `sources`   | For `sql`: `{ "NAME": "ABSOLUTE_PATH" }`; add `#SHEET` for Excel, `#TABLE` for SQLite. The SQL reads each source by NAME. Reads CSV, TSV, JSON, JSONL, Parquet, XLSX and SQLite. |
| `dateFormat` | For `sql` over CSV: the date format, for example `%d.%m.%Y`.                            |
| `path`      | A ready file: CSV, TSV, JSON (array of objects, or an object with `rows`), JSONL, XLSX.   |
| `rows`      | Inline rows as objects, for a handful of values taken from text or a document.           |
| `sheet`     | Excel sheet name; the first sheet by default.                                            |
| `delimiter` | CSV delimiter when detection fails.                                                      |
| `decimal`   | `","` when numbers use a decimal comma; assumed automatically for `;`-separated files.   |
| `columns`   | Keep only these columns, in this order.                                                  |
| `rename`    | `{ "old name": "new_name" }`, applied before everything else.                            |
| `types`     | Force a column type: `{ "inn": "string", "month": "date" }`.                             |
| `maxRows`   | Raise the 50 000-row limit up to 200 000. Prefer aggregating.                            |

Column types are inferred: `number` when every value is a number, `date` when every value is an ISO date (`2026-10`, `2026-10-31`, `2026-10-31T09:30`) or `31.10.2026`, otherwise `string`. A column of codes with leading zeros (INN, article numbers) needs `"types": { "inn": "string" }`.

Name columns in plain Latin snake_case in the SQL that produces the files (`revenue`, `order_count`); human labels belong in the spec.

## Filters

```json
{ "type": "select", "field": "region", "label": "Регион", "default": "Москва" }
{ "type": "dateRange", "field": "month", "label": "Период", "from": "2026-01-01", "to": "2026-06-30" }
```

A filter applies to every dataset that has the column, and every block re-renders against the filtered rows. `datasets: ["sales"]` limits it to the listed datasets. `dateRange` needs a `date` column. Order: the period first, then the most used dimension.

Because filters work on the embedded rows, embed the data at the grain the filters need: a region filter needs a `region` column in every dataset it should affect.

## Sections

| Key           | Meaning                                         |
| ------------- | ----------------------------------------------- |
| `title`       | Section heading.                                |
| `description` | One line under the heading.                     |
| `columns`     | Grid columns, 1–4 (default 4). Narrow screens collapse to 2 and then 1. |
| `items`       | Blocks, see `components.md`.                    |

## Keys every block accepts

| Key        | Meaning                                                               |
| ---------- | --------------------------------------------------------------------- |
| `type`     | `kpi`, `meter`, `line`, `area`, `bar`, `donut`, `scatter`, `heatmap`, `funnel`, `table`, `text`. |
| `title`    | Card title. Charts need one: it names what is plotted.                |
| `subtitle` | Smaller line under the title: unit, period, method.                   |
| `dataset`  | Dataset name. Required for everything except `text` and literal stat tiles. |
| `span`     | Grid columns the card takes, 1 to the section's `columns`. Stat tiles default to 1, other blocks to 2. |
| `height`   | Plot height in pixels for line, area, vertical bar, scatter and donut (120–900). |
| `note`     | Small text under the card: a caveat or the source of this block.      |

## Measures

Stat tiles and meters take a measure in `value`, `compare` and `target`:

| Form                                                  | Meaning                                              |
| ----------------------------------------------------- | ---------------------------------------------------- |
| `1500000`                                             | A literal number.                                    |
| `"revenue"`                                           | Sum of the column over the filtered rows.            |
| `{ "field": "revenue", "agg": "avg" }`                | Any aggregation.                                     |
| `{ "agg": "count" }`                                  | Number of rows.                                      |
| `{ "field": "client_id", "agg": "distinct" }`         | Number of distinct values.                           |
| `{ "field": "revenue", "where": { "status": "paid" } }` | Only matching rows; a list matches any of its values. |
| `{ "field": "revenue", "period": "last", "by": "month" }` | Only rows of the latest `month` value; `"previous"` takes the one before. |
| `{ "field": "balance", "agg": "last", "orderBy": "date" }` | The last value in date order.                     |
| `{ "dataset": "plan", "field": "target" }`            | A measure over another dataset.                      |
| `{ "ratio": ["revenue", "orders"] }`                  | One measure divided by another: average check, conversion, share. Each part takes any form above. |

Aggregations: `sum` (default), `avg`, `count`, `min`, `max`, `median`, `distinct`, `first`, `last`.

## Formats

`format` (values), `xFormat` and `yFormat` (axes and categories), and table column `format`:

| Format         | Shows                                 |
| -------------- | ------------------------------------- |
| `auto`         | Integers without decimals, others with up to two. |
| `integer`      | `12 480`                              |
| `decimal`      | `12 480,57`                           |
| `compact`      | `12,5 тыс.`, `3,2 млн`                |
| `percent`      | `0.253` → `25,3 %`                    |
| `percentValue` | `25.3` → `25,3 %`                     |
| `currency:RUB` | `12 480 ₽`; any ISO code: `currency:USD`, `currency:EUR` |
| `date`         | `31.10.2026`                          |
| `month`        | `окт. 2026 г.`                        |
| `year`         | `2026`                                |
| `datetime`     | `31.10.2026, 09:30`                   |
| `text`         | The value as it is.                   |

Date columns pick `month`, `year`, `date` or `datetime` automatically from their values.
