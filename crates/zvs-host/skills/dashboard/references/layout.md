# Layout

A dashboard answers a few questions in the order the reader asks them: how are we doing, how did it change, where does it come from, what are the details. Build the sections in that order.

## The standard page

1. **Header** — `title` names the subject and the scope («Продажи, 2026»), `subtitle` says what it covers, `period` the data period, `sources` in the footer.
2. **Filters** — the period first, then one or two dimensions the reader actually switches (region, product, manager). More than four filters means the dashboard is several dashboards.
3. **«Итоги»** — 3 to 6 stat tiles in one row (`columns: 4`). One hero tile at most (`hero: true`, `span: 2`). Each tile states its period and, where it helps, the comparison.
4. **«Динамика»** — one or two trend charts, `span: 2` each, or one `span: 4` for the main trend.
5. **«Структура»** — breakdowns: bars by category, a donut only for ≤ 6 parts, a heatmap for two dimensions.
6. **«Детали»** — one table (`columns: 1`) with the rows the reader will want to look up, sorted by what matters, with labelled columns.
7. **Conclusions** — a `text` block with 2–4 findings and their numbers, placed first in the section the findings are about or at the end.

## Grid

- A section has 1–4 columns. Stat tiles take 1; charts take 2 by default; a wide trend or a table takes the full row.
- Fill rows: four tiles, or two half-width charts. A lonely half-width card in the last row reads as a mistake; widen it or move it.
- Cards in one row should be of a similar kind (two charts, not a chart beside a long table).
- The screen collapses to two columns under ~1080 px and to one under ~680 px; the order of `items` is the reading order on a phone.

## Titles and text

- A chart title names the measure and the breakdown: «Выручка по регионам», not «Регионы» and not «График 1».
- Units and periods go into the `subtitle` or the format, not into every label.
- Labels in the user's language, sentence case, no abbreviations the reader would not use.
- `note` on a card for a caveat that changes how it is read («без учёта возвратов»).

## How many blocks

About 8–12 blocks fit one screenful of decisions. When the request implies more, split by audience or topic into several dashboards and say so, or put the extra detail into the table.

## Interaction is already there

Tooltips, the table view, sorting, search, the theme switch and print styles are built in. Do not add blocks to explain the controls.
