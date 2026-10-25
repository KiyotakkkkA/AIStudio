# /// script
# requires-python = ">=3.11"
# dependencies = ["duckdb==2.0.0", "openpyxl==3.1.5"]
# ///
import argparse
import json
import sys
import tempfile
from pathlib import Path

import duckdb

from _sources import SourceError, json_value, literal, parse_named, register

FORMATS = {".csv": "FORMAT csv, HEADER true", ".json": "FORMAT json, ARRAY true", ".parquet": "FORMAT parquet"}


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run DuckDB SQL over data files and save the result as CSV, JSON or Parquet."
    )
    parser.add_argument(
        "--source",
        action="append",
        default=[],
        help="NAME=ABSOLUTE_PATH; add #SHEET for Excel or #TABLE for SQLite; repeat for several files",
    )
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--sql", help="one SELECT statement over the source names")
    group.add_argument("--sql-file", help="absolute path of a file with the SELECT statement")
    parser.add_argument("--out", help="absolute path of the result: .csv, .json or .parquet")
    parser.add_argument("--preview", type=int, default=20, help="result rows to print")
    parser.add_argument("--decimal-comma", action="store_true", help="CSV numbers use a decimal comma")
    parser.add_argument("--date-format", help="CSV date format, for example %%d.%%m.%%Y")
    args = parser.parse_args()

    sql = args.sql if args.sql is not None else Path(args.sql_file).read_text(encoding="utf-8")
    sql = sql.strip().rstrip(";").strip()
    if not sql:
        print("the SQL statement is empty", file=sys.stderr)
        return 2
    out = Path(args.out) if args.out else None
    if out is not None and out.suffix.lower() not in FORMATS:
        print("--out must end with .csv, .json or .parquet", file=sys.stderr)
        return 2

    connection = duckdb.connect()
    sources = []
    try:
        with tempfile.TemporaryDirectory() as work:
            for item in args.source:
                name, path = parse_named(item)
                sources.append(register(connection, name, path, Path(work), args.decimal_comma, args.date_format))
            described = connection.execute(f"DESCRIBE {sql}").fetchall()
            columns = [{"name": row[0], "type": row[1]} for row in described]
            total = connection.execute(f"SELECT count(*) FROM ({sql}) AS result").fetchone()[0]
            cursor = connection.execute(f"SELECT * FROM ({sql}) AS result LIMIT {max(0, args.preview)}")
            header = [column[0] for column in cursor.description]
            preview = [dict(zip(header, [json_value(value) for value in row])) for row in cursor.fetchall()]
            if out is not None:
                out.parent.mkdir(parents=True, exist_ok=True)
                connection.execute(f"COPY ({sql}) TO {literal(out)} ({FORMATS[out.suffix.lower()]})")
    except (SourceError, duckdb.Error, OSError) as error:
        print(str(error), file=sys.stderr)
        return 1
    report = {
        "sources": [{key: value for key, value in source.items() if value is not None} for source in sources],
        "rows": total,
        "columns": columns,
        "preview": preview,
    }
    if out is not None:
        report["out"] = str(out)
    print(json.dumps(report, ensure_ascii=False, indent=1, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
