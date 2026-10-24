# /// script
# requires-python = ">=3.10"
# dependencies = ["python-docx==1.2.0"]
# ///
import argparse
import copy
import json
import sys
from pathlib import Path

from docx import Document
from docx.oxml import OxmlElement
from docx.oxml.ns import qn

from _blocks import (
    BLOCK_TAGS,
    W,
    block_text,
    body_blocks,
    format_ranges,
    own_sectpr,
    page_numbers,
    parse_ranges,
    prune_relationships,
)


def closing_sections(blocks: list, final) -> list:
    closing = [final] * len(blocks)
    current = final
    for index in range(len(blocks) - 1, -1, -1):
        sectpr = own_sectpr(blocks[index])
        if sectpr is not None:
            current = sectpr
        closing[index] = current
    return closing


def is_break_only(block) -> bool:
    if block.tag != f"{W}p" or block_text(block).strip():
        return False
    if any(next(block.iter(tag), None) is not None for tag in (f"{W}drawing", f"{W}pict", f"{W}object")):
        return False
    return any(node.get(qn("w:type")) == "page" for node in block.iter(f"{W}br"))


def strip_page_breaks(block, trailing: bool) -> None:
    runs = list(block.iter(f"{W}r"))
    for run in reversed(runs) if trailing else runs:
        children = [child for child in run if child.tag != f"{W}rPr"]
        for child in reversed(children) if trailing else children:
            if child.tag == f"{W}br" and child.get(qn("w:type")) == "page":
                run.remove(child)
            elif child.tag == f"{W}lastRenderedPageBreak" or (child.tag == f"{W}t" and not (child.text or "").strip()):
                continue
            else:
                return


def clear_leading_break(block) -> None:
    first = block if block.tag == f"{W}p" else next(block.iter(f"{W}p"), None)
    if first is not None:
        strip_page_breaks(first, trailing=False)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Copy a part of a .docx into a new file verbatim: formatting, images, sections, headers and footers are kept as they are."
    )
    parser.add_argument("file")
    parser.add_argument("output")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--blocks", help="0-based block indices from docx_to_markdown.py --with-index, e.g. 0-27 or 0-5,40-")
    group.add_argument("--pages", help="1-based pages, e.g. 1 or 2-4")
    parser.add_argument("--drop", action="store_true", help="remove the selection and keep everything else")
    args = parser.parse_args()

    source, target = Path(args.file), Path(args.output)
    if target.resolve() == source.resolve():
        print("Write to a new file; the input is never overwritten.", file=sys.stderr)
        return 2

    document = Document(str(source))
    body = document.element.body
    blocks = body_blocks(body)
    pages, page_source = page_numbers(document)
    if args.pages:
        wanted = set(parse_ranges(args.pages, max(pages, default=1) + 1, 1))
        selected = {index for index, page in enumerate(pages) if page in wanted}
    else:
        selected = {index for index in parse_ranges(args.blocks, len(blocks), 0) if 0 <= index < len(blocks)}
    keep = set(range(len(blocks))) - selected if args.drop else selected
    if not keep:
        print(f"Nothing would be kept; the document has {len(blocks)} blocks on {max(pages, default=0)} pages.", file=sys.stderr)
        return 2

    final = body.find(f"{W}sectPr")
    closing = closing_sections(blocks, final)
    order = sorted(keep)
    last = order[-1]
    last_section = closing[last]

    for index, block in enumerate(blocks):
        sectpr = own_sectpr(block)
        if index in keep or sectpr is None or sectpr is last_section:
            continue
        holder = next(
            (k for k in range(index - 1, -1, -1) if k in keep and closing[k] is sectpr and blocks[k].tag == f"{W}p"),
            None,
        )
        if holder is not None and own_sectpr(blocks[holder]) is None:
            blocks[holder].get_or_add_pPr()._insert_sectPr(sectpr)

    position = {id(block): index for index, block in enumerate(blocks)}
    children = list(body.iterchildren())
    for offset, child in enumerate(children):
        if child.tag == f"{W}sectPr":
            continue
        if child.tag in BLOCK_TAGS:
            if position[id(child)] not in keep:
                body.remove(child)
            continue
        neighbours = [position[id(other)] for other in children[offset + 1 :] if other.tag in BLOCK_TAGS][:1]
        if not neighbours or neighbours[0] not in keep:
            body.remove(child)

    ending = own_sectpr(blocks[last])
    if ending is not None:
        ending.getparent().remove(ending)
    if final is not None:
        body.remove(final)
    body.append(copy.deepcopy(last_section) if last_section is not None else OxmlElement("w:sectPr"))

    while len(order) > 1 and is_break_only(blocks[order[-1]]):
        body.remove(blocks[order.pop()])
    strip_page_breaks(blocks[order[-1]], trailing=True)
    clear_leading_break(blocks[order[0]])

    removed = prune_relationships(document.part)
    target.parent.mkdir(parents=True, exist_ok=True)
    document.save(str(target))

    images = sum(1 for _ in body.iter("{http://schemas.openxmlformats.org/drawingml/2006/main}blip"))
    report = {
        "output": str(target.resolve()),
        "blocksKept": len(order),
        "blocksInSource": len(blocks),
        "firstBlock": {"index": order[0], "text": block_text(blocks[order[0]]).strip()[:80]},
        "lastBlock": {"index": order[-1], "text": block_text(blocks[order[-1]]).strip()[:80]},
        "pagesKept": format_ranges(pages[index] for index in order),
        "pageSource": page_source,
        "sections": sum(1 for block in blocks if block.getparent() is body and own_sectpr(block) is not None) + 1,
        "images": images,
        "partsDropped": len(removed),
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
