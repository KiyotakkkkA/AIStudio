# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import json
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import NameObject


def load_values(source: str) -> dict:
    text = sys.stdin.read() if source == "-" else Path(source).read_text(encoding="utf-8-sig")
    data = json.loads(text)
    if isinstance(data, list):
        return {item["name"]: item["value"] for item in data if "name" in item and "value" in item}
    if not isinstance(data, dict):
        raise SystemExit("The values must be a JSON object of field name to value.")
    return data


def main() -> int:
    parser = argparse.ArgumentParser(description="Fill a PDF form from a JSON object.")
    parser.add_argument("file")
    parser.add_argument("values", help="a JSON file, or - for stdin")
    parser.add_argument("output")
    parser.add_argument("--flatten", action="store_true", help="make the filled fields read-only")
    args = parser.parse_args()

    reader = PdfReader(args.file)
    fields = reader.get_fields() or {}
    values = {str(name): value for name, value in load_values(args.values).items()}
    unknown = sorted(name for name in values if name not in fields)
    if unknown:
        print("Unknown fields: " + ", ".join(unknown), file=sys.stderr)
        print("Run form_fields.py and use the exact names it prints.", file=sys.stderr)
        return 2

    prepared: dict[str, str] = {}
    for name, value in values.items():
        if isinstance(value, bool):
            states = [str(state) for state in fields[name].get("/_States_", []) if str(state) != "/Off"]
            prepared[name] = (states[0] if states else "/Yes") if value else "/Off"
        else:
            prepared[name] = "" if value is None else str(value)

    writer = PdfWriter(clone_from=reader)
    writer.set_need_appearances_writer(True)
    for page in writer.pages:
        writer.update_page_form_field_values(page, prepared, auto_regenerate=False)
    if args.flatten:
        for page in writer.pages:
            writer.update_page_form_field_values(page, prepared, flatten=True, auto_regenerate=False)
        writer.remove_annotations(subtypes="/Widget")
        writer.root_object.pop(NameObject("/AcroForm"), None)

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as handle:
        writer.write(handle)
    print(f"Filled {len(prepared)} of {len(fields)} fields -> {output.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
