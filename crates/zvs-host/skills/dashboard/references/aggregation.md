# Aggregation with DuckDB

Shape the data with `query_data.py` before building. Each `--source NAME=PATH` becomes a view named `NAME`; the SQL is DuckDB's dialect.

```
query_data.py --source sales=C:/data/sales.xlsx#2026 --source managers=C:/data/managers.csv
  --sql "SELECT ..." --out C:/Reports/data/sales_by_month.csv
```

Write the SQL into a file and pass `--sql-file` when it is longer than a few lines. Name the output columns in snake_case.

## Choose the grain

The embedded rows are what filters slice and blocks aggregate. Aggregate to the finest grain any block or filter needs and no finer:

- Monthly charts with a region filter → one row per month × region (× product if a block breaks down by product).
- A table of the 20 largest deals → a separate small dataset with those 20 rows.
- Daily data for two years by 50 stores is 36 500 rows — fine. Order lines for a year usually are not: aggregate.

Sums, counts and averages of embedded rows are recomputed in the page; **averages of averages are wrong**. When a stat tile shows an average or a rate, embed the sum and the count and divide them in the tile: `{"ratio": ["revenue", "orders"]}` for the average check, `{"ratio": [{"field": "paid", "agg": "sum"}, {"field": "visits", "agg": "sum"}]}` for conversion. For a chart of an average, pre-compute it at exactly the grain the chart shows and do not offer a filter that cuts across that grain.

## Recipes

Monthly buckets:

```sql
SELECT date_trunc('month', order_date)::DATE AS month, region, sum(amount) AS revenue, count(*) AS orders
FROM sales
WHERE status <> 'cancelled'
GROUP BY ALL
ORDER BY month
```

Weeks start on Monday with `date_trunc('week', d)`; quarters with `date_trunc('quarter', d)`. Keep periods as dates, not as text like «Январь», so they sort and filter.

Top N and the rest:

```sql
WITH ranked AS (
  SELECT client, sum(amount) AS revenue, row_number() OVER (ORDER BY sum(amount) DESC) AS place
  FROM sales GROUP BY client
)
SELECT CASE WHEN place <= 9 THEN client ELSE 'Прочие' END AS client, sum(revenue) AS revenue
FROM ranked GROUP BY 1 ORDER BY revenue DESC
```

Share of the total and running total:

```sql
SELECT month, revenue,
       revenue / sum(revenue) OVER () AS share,
       sum(revenue) OVER (ORDER BY month) AS revenue_ytd
FROM monthly
```

Change against the previous period and the same month last year:

```sql
SELECT month, revenue,
       revenue / lag(revenue) OVER (ORDER BY month) - 1 AS mom,
       revenue / lag(revenue, 12) OVER (ORDER BY month) - 1 AS yoy
FROM monthly
```

Unpivot wide columns (Jan, Feb, …) into rows:

```sql
UNPIVOT plan ON COLUMNS(* EXCLUDE (region)) INTO NAME month_name VALUE target
```

Pivot for a wide table: `PIVOT monthly ON region USING sum(revenue) GROUP BY month`.

Join a lookup and keep unmatched rows visible:

```sql
SELECT s.*, coalesce(m.team, 'Без команды') AS team
FROM sales s LEFT JOIN managers m ON lower(trim(s.manager)) = lower(trim(m.name))
```

Fill missing months with zero only when zero is true (no sales), not when data is missing:

```sql
WITH months AS (SELECT range::DATE AS month FROM range(DATE '2026-01-01', DATE '2026-11-01', INTERVAL 1 MONTH))
SELECT months.month, coalesce(sum(s.amount), 0) AS revenue
FROM months LEFT JOIN sales s ON date_trunc('month', s.order_date) = months.month
GROUP BY ALL ORDER BY 1
```

## Cleaning

- Numbers stored as text with spaces or commas: `replace(replace(amount, ' ', ''), ',', '.')::DOUBLE`, or pass `--decimal-comma` for the whole file.
- Russian dates `31.10.2026`: `--date-format %d.%m.%Y`, or `strptime(d, '%d.%m.%Y')::DATE`.
- Excel serial dates (45230): `DATE '1899-12-30' + CAST(d AS INTEGER)`.
- Trim and unify categories: `trim(regexp_replace(region, '\s+', ' ', 'g'))`; map spelling variants with `CASE`.
- Duplicates: `SELECT DISTINCT` or `QUALIFY row_number() OVER (PARTITION BY id ORDER BY updated_at DESC) = 1`.
- Mixed currencies: convert to one with an explicit rate table, and name the rate date in a note.

## Checks before building

Run each of these and compare:

```sql
SELECT count(*), sum(amount), min(order_date), max(order_date) FROM sales            -- the source
SELECT sum(revenue), min(month), max(month) FROM read_csv('C:/Reports/data/sales_by_month.csv')  -- the result
```

- Totals match (or differ by exactly what you excluded, and you can say what).
- The period covers what the user asked for; a partial last month is noted.
- No category is empty or «None» unless that is real; no duplicate rows after joins (row counts before and after a join of a lookup must be equal).
