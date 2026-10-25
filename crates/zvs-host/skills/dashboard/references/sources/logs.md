# Logs and semi-structured text

## Structured logs

JSON lines (one object per line) read directly: `query_data.py --source log=C:/logs/app.jsonl --sql "..."`. Timestamps become timestamps; bucket them with `date_trunc('hour', ts)`.

CSV-like logs (access logs with a fixed delimiter, exports from monitoring) read as CSV; check the profile for the delimiter and the header.

## Plain-text logs

Read the file as one column of lines and parse with regular expressions in SQL:

```sql
WITH lines AS (
  SELECT unnest(string_split(replace(content, chr(13), ''), chr(10))) AS line
  FROM read_text('C:/logs/app.log')
)
SELECT
  try_strptime(regexp_extract(line, '^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})', 1), '%Y-%m-%d %H:%M:%S') AS ts,
  regexp_extract(line, '\b(INFO|WARN|ERROR)\b', 1) AS level,
  regexp_extract(line, 'service=(\S+)', 1) AS service
FROM lines
WHERE line <> ''
```

Run it with `query_data.py --sql-file C:/Reports/data/parse_log.sql --preview 20`: `read_text` reads the file itself, so no `--source` is needed.

In the preview, look at rows where the extracted fields are empty: they show the line formats the pattern misses. Count them and report them rather than dropping them silently.

## Typical aggregates

- Events per hour and level: `SELECT date_trunc('hour', ts) AS hour, level, count(*) AS events … GROUP BY ALL` → a stacked `bar` or a `line` with `series: "level"`.
- Day × hour load: `strftime(ts, '%H') AS hour, dayname(ts) AS weekday` → a `heatmap` with `yOrder` set to the weekdays.
- Top errors: group by a normalised message (`regexp_replace(message, '\d+', 'N', 'g')`) → a `table` with counts and first and last time.
- Response times: `quantile_cont(ms, 0.95)` per hour — embed the percentile at the grain the chart shows; percentiles cannot be recomputed from embedded rows.

Logs can contain personal data and secrets (tokens, e-mails, IP addresses). Leave them out of the dataset unless the user's question is about them.
