# Components

Every chart card has a «Таблица» button that shows its numbers as a table, a tooltip on hover and keyboard focus, and a legend when it has two or more series. Colours are assigned to series in a fixed order by their total over the unfiltered data, so a filter never repaints a series.

## Choosing a block

| The reader must…                               | Use                                   | Not                         |
| ---------------------------------------------- | ------------------------------------- | --------------------------- |
| See one current number, maybe with its change  | `kpi`                                 | a chart with one bar        |
| See progress towards a target                  | `meter`                               | a donut with two slices     |
| Follow a trend over time                       | `line`; `area` for a single total     | bars for 30+ points         |
| Compare categories                             | `bar` (horizontal for long names or many categories) | a donut              |
| See parts of a whole at a glance (≤ 6 parts)   | `donut`, or a stacked `bar`           | a donut for close values    |
| Compare composition across time or categories  | `bar` with `series` and `stacked`     | several donuts              |
| See a relation between two measures            | `scatter`                             | two lines on one axis       |
| Find patterns in a grid (day × hour, region × month) | `heatmap`                       | a table of 100 numbers      |
| Follow conversion through ordered stages       | `funnel`                              | a bar sorted by value       |
| Look up exact values                           | `table`                               | a chart with labels on every point |
| Explain, conclude, warn                        | `text`                                | a title that is a sentence  |

Never put two measures with different scales on one chart; use two charts side by side.

## `kpi` — stat tile

```json
{ "type": "kpi", "label": "Выручка за месяц", "dataset": "sales",
  "value":   { "field": "revenue", "period": "last", "by": "month" },
  "compare": { "field": "revenue", "period": "previous", "by": "month" },
  "compareLabel": "к прошлому месяцу", "format": "currency:RUB", "goodWhen": "up",
  "spark": { "x": "month", "y": "revenue" }, "hero": false }
```

- `label` (or `title`) is required: sentence case, no colon, with the period.
- `compare` adds a delta; `deltaMode`: `percent` (default) or `absolute`. `goodWhen`: `up` (default) or `down` (costs, churn, errors) decides the colour; the arrow and sign always show the direction.
- `spark` draws the last 12 points of `y` by `x` (`points` changes the count; `agg` the aggregation).
- `hero: true` makes the number large. One hero per page, usually with `span: 2`.
- Literal values need no dataset: `{ "type": "kpi", "label": "Сотрудники", "value": 128 }`.

## `meter` — progress to a target

```json
{ "type": "meter", "title": "План продаж на год", "dataset": "sales", "value": "revenue", "target": 150000000, "format": "currency:RUB" }
```

`value` and `target` are measures; the card shows the value, the target and the share.

## `line` and `area` — trends

```json
{ "type": "line", "title": "Выручка по продуктам", "dataset": "sales", "x": "month", "y": "revenue", "series": "product", "format": "currency:RUB" }
{ "type": "line", "title": "План и факт", "dataset": "sales", "x": "month", "y": [ { "field": "fact", "label": "Факт" }, { "field": "plan", "label": "План" } ] }
{ "type": "area", "title": "Выручка всего", "dataset": "sales", "x": "month", "y": "revenue", "format": "currency:RUB" }
```

- Rows are grouped by `x` (and `series`) and `y` is aggregated with `agg` (default `sum`). `agg: "count"` without `y` counts rows.
- Several series: either `series` (a column whose values become series, long format) or a list in `y` (several columns, wide format), not both. At most 8 series; past 7 the smallest fold into «Прочее».
- `x` is sorted by value for dates and numbers. `sort: "none"` keeps the data order for text categories.
- `area` with `series` and `stacked: true` stacks the series.
- With up to 4 series the last value of each is labelled at the line end; `endLabels: false` turns it off.
- `xLabel` names the first column of the table view.

## `bar` — categories

```json
{ "type": "bar", "title": "Выручка по регионам", "dataset": "sales", "x": "region", "y": "revenue", "format": "currency:RUB", "limit": 10 }
{ "type": "bar", "title": "Заказы по месяцам", "dataset": "sales", "x": "month", "y": "orders", "series": "channel", "stacked": true }
```

- Text categories sort by value, largest first (`sort`: `desc`, `asc`, `x` for the category order, `none`); dates and numbers sort by `x`.
- `limit: N` keeps N−1 categories and folds the rest into «Прочее».
- Orientation is horizontal automatically for more than 7 categories or long names; `orientation` forces `vertical` or `horizontal`.
- Single-series bars show their value at the tip; `valueLabels: false` hides them.
- `series` without `stacked` draws grouped bars; keep groups to 2–3 series.

## `donut` — parts of a whole

```json
{ "type": "donut", "title": "Структура выручки", "dataset": "sales", "label": "product", "value": "revenue", "format": "currency:RUB" }
```

At most 6 segments (`limit` lowers it); the rest folds into «Прочее». Only positive values. The list beside the ring shows each value and share. `centerLabel` replaces «Всего» under the total.

## `scatter` — relation of two measures

```json
{ "type": "scatter", "title": "Средний чек и число заказов", "dataset": "clients", "x": "orders", "y": "avg_check", "series": "segment", "label": "client", "xLabel": "Заказы", "yLabel": "Средний чек, ₽" }
```

One point per row (up to 5 000). At most 3 series; more fold into «Прочее». `label` names the point in the tooltip. Always set `xLabel` and `yLabel`.

## `heatmap` — a grid of values

```json
{ "type": "heatmap", "title": "Обращения по дням и часам", "dataset": "tickets", "x": "hour", "y": "weekday", "value": "tickets", "yOrder": ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"] }
```

Cells aggregate `value` with `agg`; one blue ramp from low to high with a scale under the title. Text categories keep the data order; `xOrder` and `yOrder` fix it, `sortCategories: true` sorts them. Values are printed in cells that fit them; `cellLabels: false` hides them. `cellHeight` sets the row height (default 28).

## `funnel` — ordered stages

```json
{ "type": "funnel", "title": "Воронка продаж", "dataset": "stages", "label": "stage", "value": "users", "order": ["Визит", "Регистрация", "Пробный период", "Оплата"] }
```

Stages keep the data order unless `order` is given. Each stage shows its value, the share of the first stage and the conversion from the previous one.

## `table` — exact values

```json
{ "type": "table", "title": "Крупнейшие сделки", "dataset": "deals",
  "columns": [ { "field": "client", "label": "Клиент" }, { "field": "amount", "label": "Сумма", "format": "currency:RUB", "bar": true }, { "field": "closed", "label": "Дата" } ],
  "sort": { "field": "amount", "dir": "desc" }, "limit": 20 }
```

- `columns` lists fields or objects with `field`, `label`, `format`, `bar` (an inline bar proportional to the largest value). Without `columns`, all columns are shown with their raw names — always give labels.
- Headers sort on click; a search box appears for more than 10 rows (`search: false` hides it); `limit` rows are shown first (default 50) with «Показать все».

## `text` — conclusions and caveats

```json
{ "type": "text", "title": "Выводы", "text": "Выручка выросла на **12 %** за квартал.\n\n- Москва — 41 % выручки\n- Внедрение растёт быстрее остальных продуктов" }
```

Paragraphs are separated by an empty line, list lines start with `- `, `**bold**` is supported. Write the conclusion the reader should draw, with the numbers; do not repeat the chart titles.
