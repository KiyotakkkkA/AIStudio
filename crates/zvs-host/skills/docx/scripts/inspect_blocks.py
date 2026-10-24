# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import copy
import difflib
import hashlib
import sys

from docx import Document
from lxml import etree

from _blocks import R, W, block_text, body_blocks, own_sectpr, page_numbers, parse_ranges

NOISE = ("lastRenderedPageBreak", "proofErr", "bookmarkStart", "bookmarkEnd", "noProof", "sectPr")
VOLATILE = ("paraId", "textId", "anchorId", "editId")


def local(name: str) -> str:
    return name.rsplit("}", 1)[-1]


def describe(element) -> str:
    if element is None:
        return ""
    parts = []
    for child in element:
        name = local(child.tag)
        if name in ("rPr", "sectPr", "pPrChange", "rPrChange") or name.startswith("rsid"):
            continue
        attributes = ",".join(f"{local(key)}:{value}" for key, value in child.attrib.items() if not local(key).startswith("rsid"))
        nested = describe(child)
        parts.append(name + (f"={attributes}" if attributes else "") + (f"{{{nested}}}" if nested else ""))
    return " ".join(parts)


def fingerprint(block) -> str:
    clean = copy.deepcopy(block)
    for node in list(clean.iter()):
        if not isinstance(node.tag, str):
            continue
        if local(node.tag) in NOISE and node.getparent() is not None:
            node.getparent().remove(node)
            continue
        for key in list(node.attrib):
            if local(key).startswith("rsid") or local(key) in VOLATILE or key.startswith(R):
                del node.attrib[key]
    return hashlib.sha1(etree.tostring(clean, method="c14n", exclusive=True)).hexdigest()[:12]


def summary(block, styles: dict[str, str]) -> str:
    text = " ".join(block_text(block).split())
    images = sum(1 for _ in block.iter("{http://schemas.openxmlformats.org/drawingml/2006/main}blip"))
    tail = (f" img={images}" if images else "") + (" +section-break" if own_sectpr(block) is not None else "")
    quoted = f'"{text[:70]}{"…" if len(text) > 70 else ""}"'
    if block.tag == f"{W}tbl":
        rows = block.findall(f"{W}tr")
        columns = len(block.findall(f"{W}tblGrid/{W}gridCol"))
        style = block.find(f"{W}tblPr/{W}tblStyle")
        name = styles.get(style.get(f"{W}val"), style.get(f"{W}val")) if style is not None else "-"
        return f"TABLE {len(rows)}x{columns} style={name}{tail} {quoted}"
    if block.tag == f"{W}sdt":
        return f"CONTENT-CONTROL paragraphs={sum(1 for _ in block.iter(f'{W}p'))}{tail} {quoted}"
    properties = block.find(f"{W}pPr")
    style = properties.find(f"{W}pStyle") if properties is not None else None
    name = styles.get(style.get(f"{W}val"), style.get(f"{W}val")) if style is not None else "Normal"
    runs = []
    for run in block.iter(f"{W}r"):
        if not "".join(node.text or "" for node in run.iter(f"{W}t")).strip():
            continue
        described = describe(run.find(f"{W}rPr")) or "-"
        if described not in runs:
            runs.append(described)
    paragraph = describe(properties).replace(f"pStyle=val:{style.get(f'{W}val')}", "").strip() if style is not None else describe(properties)
    return f"P style={name} [{paragraph or '-'}] runs=[{' | '.join(runs) or '-'}]{tail} {quoted}"


def load(path: str, spec: str | None):
    document = Document(path)
    styles = {style.style_id: style.name for style in document.styles}
    blocks = body_blocks(document.element.body)
    pages, _ = page_numbers(document)
    indices = [index for index in parse_ranges(spec, len(blocks), 0) if 0 <= index < len(blocks)] if spec else list(range(len(blocks)))
    return [(index, pages[index], blocks[index], styles) for index in indices]


def line(entry) -> str:
    index, page, block, styles = entry
    return f"[{index} p.{page}] {summary(block, styles)}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Show the formatting of every block, or compare two documents block by block.")
    parser.add_argument("file")
    parser.add_argument("--blocks", help="0-based block range of FILE, e.g. 0-21")
    parser.add_argument("--start", type=int, default=0)
    parser.add_argument("--limit", type=int, default=120)
    parser.add_argument("--against", help="another .docx to compare with, e.g. the source of an edit")
    parser.add_argument("--against-blocks", help="0-based block range of the --against document")
    args = parser.parse_args()

    entries = load(args.file, args.blocks)
    if not args.against:
        shown = entries[args.start : args.start + max(args.limit, 1)]
        print("\n".join(line(entry) for entry in shown))
        if args.start + len(shown) < len(entries):
            print(f"[{len(shown)} of {len(entries)} blocks shown; continue with --start {args.start + len(shown)}]")
        return 0

    others = load(args.against, args.against_blocks)
    mine = [fingerprint(entry[2]) for entry in entries]
    theirs = [fingerprint(entry[2]) for entry in others]
    matcher = difflib.SequenceMatcher(a=theirs, b=mine, autojunk=False)
    identical = sum(size for _, _, size in matcher.get_matching_blocks())
    print(f"{identical} of {len(entries)} blocks are identical to {len(others)} blocks of the other document.")
    reported = 0
    for tag, a_start, a_end, b_start, b_end in matcher.get_opcodes():
        if tag == "equal":
            continue
        for entry in others[a_start:a_end]:
            print(f"- other {line(entry)}")
        for entry in entries[b_start:b_end]:
            print(f"+ this  {line(entry)}")
        reported += 1
        if reported >= 40:
            print("[more differences not shown; narrow the ranges]")
            break
    if not reported:
        print("No differences in text or formatting.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
