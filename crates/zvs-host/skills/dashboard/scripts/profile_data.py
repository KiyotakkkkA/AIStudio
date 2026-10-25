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

from _sources import (
    EXCEL_EXT,
    SQLITE_EXT,
    SourceError,
    excel_sheets,
    ident,
    json_value,
    register,
    split_source,
    sqlite_tables,
)


def profile(connection, sample: int, top: int) -> dict:
    total = connection.execute("SELECT count(*) FROM data").fetchone()[0]
    cursor = connection.execute("SUMMARIZE SELECT * FROM data")
    names = [column[0] for column in cursor.description]
    columns = []
    for row in cursor.fetchall():
        record = dict(zip(names, row))
        column = {
            "name": record.get("column_name"),
            "type": record.get("column_type"),
            "nulls": record.get("null_percentage"),
            "distinct": record.get("approx_unique"),
            "min": record.get("min"),
            "max": record.get("max"),
        }
        if record.get("avg") is not None:
            column["avg"] = record.get("avg")
        type_name = str(column["type"]).upper()
        if type_name.startswith("VARCHAR") and (column["distinct"] or 0) <= 50:
            values = connection.execute(
                f"SELECT {ident(column['name'])} AS value, count(*) AS n FROM data "
                f"GROUP BY 1 ORDER BY 2 DESC LIMIT {top}"
            ).fetchall()
            column["top"] = [[value, count] for value, count in values]
        columns.append({key: json_value(value) for key, value in column.items()})
    head = connection.execute(f"SELECT * FROM data LIMIT {sample}")
    header = [column[0] for column in head.description]
    rows = [dict(zip(header, [json_value(value) for value in row])) for row in head.fetchall()]
    return {"rows": total, "columns": columns, "sample": rows}


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Profile a data file: row count, column types, nulls, ranges, frequent values and sample rows."
    )
    parser.add_argument("source", help="absolute path; add #SHEET for Excel or #TABLE for SQLite")
    parser.add_argument("--sample", type=int, default=5, help="sample rows to print")
    parser.add_argument("--top", type=int, default=8, help="frequent values per text column")
    parser.add_argument("--decimal-comma", action="store_true", help="CSV numbers use a decimal comma")
    parser.add_argument("--date-format", help="CSV date format, for example %%d.%%m.%%Y")
    args = parser.parse_args()

    path_text, part = split_source(args.source)
    suffix = Path(path_text).suffix.lower()
    report: dict = {"source": args.source}
    try:
        if suffix in SQLITE_EXT and part is None:
            report["tables"] = sqlite_tables(Path(path_text))
            report["hint"] = "profile one table with PATH#TABLE"
            print(json.dumps(report, ensure_ascii=False, indent=1))
            return 0
        if suffix in EXCEL_EXT:
            report["sheets"] = excel_sheets(Path(path_text))
        connection = duckdb.connect()
        with tempfile.TemporaryDirectory() as work:
            info = register(connection, "data", args.source, Path(work), args.decimal_comma, args.date_format)
            report.update({key: value for key, value in info.items() if key not in ("name", "path", "part")})
            report.update(profile(connection, args.sample, args.top))
    except (SourceError, duckdb.Error) as error:
        print(str(error), file=sys.stderr)
        return 1
    print(json.dumps(report, ensure_ascii=False, indent=1, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
