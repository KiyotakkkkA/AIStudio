# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

PAYLOAD = re.compile(r'<script type="application/json" id="zvs-dashboard-data">(.*?)</script>', re.S)
EXTERNAL = [
    (re.compile(r"""(?:src|href)\s*=\s*["']?\s*(?:https?:)?//""", re.I), "a resource loaded from the network"),
    (re.compile(r"""url\(\s*["']?\s*(?:https?:)?//""", re.I), "a stylesheet url() pointing to the network"),
    (re.compile(r"@import", re.I), "a stylesheet @import"),
    (re.compile(r"\bfetch\s*\(|XMLHttpRequest|new\s+WebSocket|navigator\.sendBeacon"), "a network call in a script"),
]


def main() -> int:
    parser = argparse.ArgumentParser(description="Check that a dashboard HTML file is self-contained and complete.")
    parser.add_argument("file", help="absolute path of the HTML file")
    args = parser.parse_args()
    path = Path(args.file)
    if not path.is_file():
        print(f"not a file: {path}", file=sys.stderr)
        return 2
    text = path.read_text(encoding="utf-8", errors="replace")
    problems: list[str] = []
    warnings: list[str] = []
    title = re.search(r"<title>(.*?)</title>", text, re.S)
    if not title or not title.group(1).strip():
        problems.append("the page has no <title>")
    match = PAYLOAD.search(text)
    data = None
    if not match:
        problems.append("no dashboard payload; build the page with build_dashboard.py")
    else:
        try:
            data = json.loads(match.group(1).replace("<\\/", "</"))
        except json.JSONDecodeError as error:
            problems.append(f"the payload is not valid JSON: {error}")
    outside = PAYLOAD.sub("", text)
    for pattern, label in EXTERNAL:
        if pattern.search(outside):
            problems.append(f"found {label}; the page must work offline")
    report: dict = {"file": str(path), "kib": round(len(text.encode("utf-8")) / 1024, 1)}
    if isinstance(data, dict):
        datasets = data.get("datasets") or {}
        kinds = Counter()
        for index, section in enumerate(data.get("sections") or []):
            for position, item in enumerate(section.get("items") or []):
                kinds[item.get("type")] += 1
                name = item.get("dataset")
                if name is not None and name not in datasets:
                    problems.append(f"sections[{index}].items[{position}]: missing dataset {name!r}")
                if name in datasets and not datasets[name].get("rows"):
                    warnings.append(f"sections[{index}].items[{position}]: dataset {name!r} is empty")
        if not kinds:
            problems.append("the dashboard has no components")
        report["components"] = dict(kinds)
        report["datasets"] = {name: len(value.get("rows") or []) for name, value in datasets.items()}
    report["ok"] = not problems
    report["problems"] = problems
    report["warnings"] = warnings
    print(json.dumps(report, ensure_ascii=False, indent=1))
    return 0 if not problems else 1


if __name__ == "__main__":
    sys.exit(main())
