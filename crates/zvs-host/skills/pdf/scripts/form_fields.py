# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import json
import sys
from pathlib import Path

from pypdf import PdfReader
from pypdf.generic import ArrayObject


def kind_of(field: dict) -> str:
    kind = field.get("/FT")
    flags = int(field.get("/Ff", 0) or 0)
    if kind == "/Btn":
        if flags & (1 << 16):
            return "button"
        return "radio" if flags & (1 << 15) else "checkbox"
    if kind == "/Ch":
        return "combo" if flags & (1 << 17) else "list"
    if kind == "/Sig":
        return "signature"
    return "text"


def options(field: dict, kind: str) -> list[str]:
    if kind in ("combo", "list"):
        values = []
        for option in field.get("/Opt", []) or []:
            if isinstance(option, (list, ArrayObject)) and option:
                values.append(str(option[-1]))
            else:
                values.append(str(option))
        return values
    if kind in ("checkbox", "radio"):
        states = field.get("/_States_", []) or []
        return [str(state) for state in states if str(state) != "/Off"]
    return []


def pages_of(reader: PdfReader) -> dict[str, int]:
    located: dict[str, int] = {}
    for number, page in enumerate(reader.pages, start=1):
        for annotation in page.get("/Annots", []) or []:
            widget = annotation.get_object()
            name = widget.get("/T")
            parent = widget.get("/Parent")
            if name is None and parent is not None:
                name = parent.get_object().get("/T")
            if name is not None:
                located.setdefault(str(name), number)
    return located


def main() -> int:
    parser = argparse.ArgumentParser(description="List the fillable fields of a PDF form as JSON.")
    parser.add_argument("file")
    parser.add_argument("--output", help="write the JSON to this file as well")
    args = parser.parse_args()

    reader = PdfReader(args.file)
    fields = reader.get_fields() or {}
    pages = pages_of(reader)
    result = []
    for name, field in fields.items():
        kind = kind_of(field)
        if kind == "button":
            continue
        value = field.get("/V")
        result.append(
            {
                "name": name,
                "type": kind,
                "value": None if value is None else str(value),
                "options": options(field, kind),
                "page": pages.get(name.split(".")[-1]) or pages.get(name),
                "required": bool(int(field.get("/Ff", 0) or 0) & 2),
                "readOnly": bool(int(field.get("/Ff", 0) or 0) & 1),
                "tooltip": str(field["/TU"]) if "/TU" in field else None,
            }
        )
    if not result:
        print("The document has no fillable fields. Filling it needs text placed over the page instead.")
        return 0
    text = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        Path(args.output).write_text(text, encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
