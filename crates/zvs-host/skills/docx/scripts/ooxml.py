# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
import argparse
import json
import re
import sys
import zipfile
from pathlib import Path
from xml.dom import minidom

XML_SUFFIXES = (".xml", ".rels")


def pretty(data: bytes) -> bytes:
    try:
        return minidom.parseString(data).toprettyxml(indent="  ", encoding="utf-8")
    except Exception:
        return data


def compact(data: bytes) -> bytes:
    try:
        text = minidom.parseString(data).toxml(encoding="utf-8").decode("utf-8")
    except Exception:
        return data
    text = re.sub(r">[ \t]*\r?\n\s*<", "><", text)
    return text.encode("utf-8")


def unpack(source: Path, target: Path) -> int:
    if target.exists() and any(target.iterdir()):
        print(f"{target} is not empty.", file=sys.stderr)
        return 2
    with zipfile.ZipFile(source) as archive:
        for item in archive.infolist():
            if item.is_dir():
                continue
            destination = target / item.filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            data = archive.read(item)
            destination.write_bytes(pretty(data) if item.filename.endswith(XML_SUFFIXES) else data)
    print(f"Unpacked {source} -> {target.resolve()}")
    return 0


def pack(source: Path, target: Path) -> int:
    files = sorted(path for path in source.rglob("*") if path.is_file())
    names = [path.relative_to(source).as_posix() for path in files]
    if "[Content_Types].xml" not in names:
        print("[Content_Types].xml is missing: this is not an unpacked Office file.", file=sys.stderr)
        return 2
    target.parent.mkdir(parents=True, exist_ok=True)
    ordered = sorted(zip(names, files), key=lambda pair: pair[0] != "[Content_Types].xml")
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, path in ordered:
            data = path.read_bytes()
            archive.writestr(name, compact(data) if name.endswith(XML_SUFFIXES) else data)
    print(f"Packed {len(files)} parts -> {target.resolve()}")
    return 0


def show(source: Path, part: str, find: str | None, context: int, start: int, limit: int) -> int:
    with zipfile.ZipFile(source) as archive:
        if part not in archive.namelist():
            print("Parts: " + ", ".join(name for name in archive.namelist() if name.endswith(XML_SUFFIXES)))
            return 2
        lines = pretty(archive.read(part)).decode("utf-8", errors="replace").splitlines()
    if find:
        hits = [index for index, line in enumerate(lines) if find in line]
        if not hits:
            print("No match.")
            return 0
        shown = set()
        for hit in hits[:20]:
            for index in range(max(0, hit - context), min(len(lines), hit + context + 1)):
                if index not in shown:
                    shown.add(index)
                    print(f"{index + 1:>6}  {lines[index]}")
            print("   ...")
        if len(hits) > 20:
            print(f"{len(hits)} matches, the first 20 shown.")
        return 0
    end = min(len(lines), start + limit)
    for index in range(start, end):
        print(f"{index + 1:>6}  {lines[index]}")
    if end < len(lines):
        print(f"[lines {start + 1}-{end} of {len(lines)}; continue with --start {end}]")
    return 0


def replace(source: Path, target: Path, part: str, pairs_source: str) -> int:
    text = sys.stdin.read() if pairs_source == "-" else Path(pairs_source).read_text(encoding="utf-8-sig")
    pairs = json.loads(text)
    if not isinstance(pairs, dict) or not pairs:
        print('Replacements must be a non-empty JSON object of {"old xml": "new xml"}.', file=sys.stderr)
        return 2
    if target.resolve() == source.resolve():
        print("Write to a new file; the input is never overwritten.", file=sys.stderr)
        return 2
    with zipfile.ZipFile(source) as archive:
        items = [(item, archive.read(item)) for item in archive.infolist()]
    counts = {}
    for index, (item, data) in enumerate(items):
        if item.filename != part:
            continue
        xml = compact(data).decode("utf-8")
        for old, new in pairs.items():
            counts[old] = xml.count(old)
            xml = xml.replace(old, new)
        try:
            minidom.parseString(xml.encode("utf-8"))
        except Exception as error:
            print(f"The result is not well-formed XML: {error}", file=sys.stderr)
            return 2
        items[index] = (item, xml.encode("utf-8"))
    if not counts:
        print(f"{part} is not in the package.", file=sys.stderr)
        return 2
    if not any(counts.values()):
        print(json.dumps({"replaced": counts}, ensure_ascii=False, indent=2))
        print("Nothing matched; no file was written. Copy the fragment from one line of `show` output.", file=sys.stderr)
        return 2
    target.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
        for item, data in items:
            archive.writestr(item, data)
    print(json.dumps({"replaced": counts, "output": str(target.resolve())}, ensure_ascii=False, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect and edit the XML inside .docx, .xlsx and .pptx files.")
    commands = parser.add_subparsers(dest="command", required=True)
    unpack_parser = commands.add_parser("unpack")
    unpack_parser.add_argument("file")
    unpack_parser.add_argument("dir")
    pack_parser = commands.add_parser("pack")
    pack_parser.add_argument("dir")
    pack_parser.add_argument("output")
    show_parser = commands.add_parser("show")
    show_parser.add_argument("file")
    show_parser.add_argument("part", nargs="?", default="word/document.xml")
    show_parser.add_argument("--find")
    show_parser.add_argument("--context", type=int, default=6)
    show_parser.add_argument("--start", type=int, default=0)
    show_parser.add_argument("--limit", type=int, default=400)
    replace_parser = commands.add_parser("replace")
    replace_parser.add_argument("file")
    replace_parser.add_argument("output")
    replace_parser.add_argument("--part", default="word/document.xml")
    replace_parser.add_argument("--pairs", default="-", help="a JSON file, or - for stdin")
    args = parser.parse_args()

    if args.command == "unpack":
        return unpack(Path(args.file), Path(args.dir))
    if args.command == "pack":
        return pack(Path(args.dir), Path(args.output))
    if args.command == "show":
        return show(Path(args.file), args.part, args.find, args.context, args.start, args.limit)
    return replace(Path(args.file), Path(args.output), args.part, args.pairs)


if __name__ == "__main__":
    sys.exit(main())
