# Refresh

Every dashboard the app saves can be refreshed: the builder keeps the spec beside the page, and «Обновить» on the page or in «Галерея» runs the dataset queries again and replaces the data in place. Filters, the theme and the scroll position stay.

- **What changes on refresh.** A `sql` dataset reads its `sources` again, so a dashboard over the user's own files or SQLite databases follows them. A `path` to a file in the chat folder is a frozen copy: the builder saves it beside the page and warns. For a dashboard that should stay current, read the original files with `sql`.
- **`refresh.interval`.** Set it when the user wants the page to follow changing data («в реальном времени», «обновлять каждые 5 минут»); choose the slowest interval that serves them, never below what the source changes at. The page refreshes only while it is open and visible, and the user can change or pause the interval.
- **`fetch`.** For data that comes from an API, write a script that takes the path of its output file as its **last argument** and writes JSON (an array of objects, or an object with `rows`) or CSV there; tokens come from environment variables only. Run it once with `shell.run` (`argv: ["uv", "run", "chat:_fetch_crm.py", "_crm.json"]` or `node`), read its output in a dataset, and list it:

```json
"fetch": [ { "script": "C:/…/work/_fetch_crm.py", "output": "C:/…/work/_crm.json" } ],
"datasets": { "deals": { "sources": { "crm": "C:/…/work/_crm.json" }, "sql": "SELECT …" } }
```

The builder copies the script and its output beside the page; a dataset that read the output reads the copy from then on. The app runs a `fetch` script only after the user approves its exact text in «Галерея»; say so in the report. Only `.py` and `.js` scripts are accepted. Connector (MCP) data cannot be refreshed by the app: a dashboard built from it is a snapshot, so say that it shows the data at the time of building.
