# Exploring the raw data

Explore before asking the user anything and before aggregating. The goal is to know the data well enough to propose a concrete dashboard and to ask only the questions the data cannot answer. Everything here is read-only.

## 1. Inventory the sources

- List what the request points to: named files, a folder (`fs.list` with a glob such as `**/*.{csv,xlsx}`), a database, a knowledge store, a connector.
- Profile each file with `profile_data.py`. For Excel, look at every sheet the profile lists; for SQLite, at every table that looks relevant.
- Several similar files (`sales_2025.xlsx`, `sales_2026.xlsx`, monthly exports): check whether they share columns and whether their periods overlap or leave gaps.

## 2. Understand each table

From the profile, write down for yourself:

| Question                         | Where to see it                                               |
| -------------------------------- | ------------------------------------------------------------- |
| What one row is                  | Sample rows; a column that is unique per row (`order_id`)     |
| The date column and its grain    | Type `DATE`/`TIMESTAMP`, `min`/`max`, distinct count          |
| Measures                         | Numeric columns that make sense to sum or average             |
| Dimensions                       | Text columns with a few to a few hundred distinct values      |
| Identifiers and codes            | Many distinct values, leading zeros, no sense in summing      |
| Data quality                     | `nulls`, numbers typed as `VARCHAR`, odd `min`/`max`          |

## 3. Run the quick queries

Keep each query small; use `--preview` to read results, do not write files yet.

```sql
-- period and coverage: months with rows, gaps show up as missing months
SELECT date_trunc('month', order_date) AS month, count(*) AS rows, sum(amount) AS amount
FROM sales GROUP BY 1 ORDER BY 1

-- totals to reconcile against later
SELECT count(*) AS rows, count(DISTINCT order_id) AS orders, sum(amount) AS amount FROM sales

-- the shape of each dimension: how many values, which dominate
SELECT region, count(*) AS rows, sum(amount) AS amount FROM sales GROUP BY 1 ORDER BY 3 DESC LIMIT 15

-- duplicates on what should be a key
SELECT order_id, count(*) AS n FROM sales GROUP BY 1 HAVING n > 1 ORDER BY n DESC LIMIT 10

-- outliers and suspicious values
SELECT min(amount), quantile_cont(amount, 0.5) AS median, quantile_cont(amount, 0.99) AS p99, max(amount),
       count(*) FILTER (WHERE amount < 0) AS negative, count(*) FILTER (WHERE amount IS NULL) AS empty
FROM sales

-- spelling variants of the same category
SELECT lower(trim(region)) AS normalised, list(DISTINCT region) AS variants
FROM sales GROUP BY 1 HAVING count(DISTINCT region) > 1
```

Also look for: a total or subtotal row inside the data (a row whose amount equals the sum of the others), test or cancelled records (`status`, `is_test`), several currencies, a partial first or last period, and columns that look alike (`amount`, `amount_net`, `amount_vat`).

## 4. Turn findings into a plan and questions

Write one message to the user:

1. **What was found** — sources, rows, the period and its gaps, the main measures and dimensions, problems (duplicates, empty values, a partial month).
2. **What you propose** — 3–6 headline numbers with their periods, the trends and breakdowns, the filters.
3. **What is open** — only questions the data could not settle, each with the default you will use:
   - which of several similar measures to use («выручка с НДС или без — по умолчанию без»);
   - whether to keep a partial period, test rows, cancellations or returns;
   - what an unexplained code or abbreviation means;
   - the audience and the decision, if the request does not say and it changes what goes first;
   - a plan or target to compare with, if the data has none.

Do not ask what the data already shows (the period, the regions, the products), about layout, colours or chart types, or for permission to do what was requested.

When nothing important is open, state the plan in two or three lines and continue without waiting. When the user answers only some questions, use the stated defaults for the rest.

## Examples

Weak (asked before looking):

> За какой период построить дашборд? Какие показатели вам важны? По каким разрезам?

Strong (asked after exploring):

> В `sales_2026.xlsx` 18 420 строк за январь–октябрь 2026: выручка, число заказов, 5 регионов, 4 продукта, 37 менеджеров. Октябрь неполный (до 24-го), 112 строк дублируются по номеру заказа.
>
> Предлагаю: выручку и заказы за последний полный месяц с изменением к предыдущему, динамику по месяцам и продуктам, выручку по регионам, топ-10 менеджеров и таблицу заказов; фильтры — период и регион.
>
> Уточните, пожалуйста:
> 1. Октябрь показывать как неполный месяц или исключить? По умолчанию — показать с пометкой.
> 2. Дубли по номеру заказа — это повторные выгрузки? По умолчанию — оставить одну строку из каждой пары.
