from docx.oxml.ns import qn

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
R = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}"
BLOCK_TAGS = (f"{W}p", f"{W}tbl", f"{W}sdt")
CONTENT_TAGS = (f"{W}t", f"{W}drawing", f"{W}pict", f"{W}object", f"{W}tab", f"{W}sym")
PRUNABLE = ("/image", "/hyperlink", "/header", "/footer", "/oleObject", "/package", "/chart", "/diagram", "/video", "/audio", "/media")


def body_blocks(body) -> list:
    return [child for child in body.iterchildren() if child.tag in BLOCK_TAGS]


def block_text(block) -> str:
    return "".join(node.text or "" for node in block.iter(f"{W}t"))


def own_sectpr(block):
    if block.tag != f"{W}p":
        return None
    properties = block.find(f"{W}pPr")
    return None if properties is None else properties.find(f"{W}sectPr")


def section_starts_new_page(sectpr) -> bool:
    kind = sectpr.find(f"{W}type")
    return kind is None or kind.get(qn("w:val")) != "continuous"


def styles_breaking_before(document) -> set[str]:
    try:
        root = document.styles.element
    except Exception:
        return set()
    parents: dict[str, str | None] = {}
    direct: dict[str, bool] = {}
    default = None
    for style in root.findall(f"{W}style"):
        if style.get(qn("w:type")) != "paragraph":
            continue
        identifier = style.get(qn("w:styleId"))
        based = style.find(f"{W}basedOn")
        parents[identifier] = None if based is None else based.get(qn("w:val"))
        flag = style.find(f"{W}pPr/{W}pageBreakBefore")
        if flag is not None:
            direct[identifier] = flag.get(qn("w:val"), "1") not in ("0", "false", "off")
        if style.get(qn("w:default")) in ("1", "true"):
            default = identifier
    breaking = set()
    for identifier in parents:
        current, seen = identifier, set()
        while current is not None and current not in seen:
            seen.add(current)
            if current in direct:
                if direct[current]:
                    breaking.add(identifier)
                break
            current = parents.get(current)
    if default in breaking:
        breaking.add("")
    return breaking


def paragraph_style(block) -> str:
    style = block.find(f"{W}pPr/{W}pStyle")
    return "" if style is None else style.get(qn("w:val"), "")


def breaks_before(block, breaking_styles: set[str]) -> bool:
    if block.tag != f"{W}p":
        first = next(block.iter(f"{W}p"), None)
        return first is not None and breaks_before(first, breaking_styles)
    flag = block.find(f"{W}pPr/{W}pageBreakBefore")
    if flag is not None:
        return flag.get(qn("w:val"), "1") not in ("0", "false", "off")
    return paragraph_style(block) in breaking_styles


def is_page_break(node, rendered: bool) -> bool:
    if rendered:
        return node.tag == f"{W}lastRenderedPageBreak"
    return node.tag == f"{W}br" and node.get(qn("w:type")) == "page"


def break_positions(block, rendered: bool) -> tuple[bool, int]:
    leading, trailing, seen = False, 0, False
    for node in block.iter():
        if node.tag in CONTENT_TAGS and (node.tag != f"{W}t" or (node.text or "").strip()):
            seen = True
        elif is_page_break(node, rendered):
            if seen:
                trailing += 1
            else:
                leading = True
    return leading, trailing


def page_numbers(document) -> tuple[list[int], str]:
    body = document.element.body
    blocks = body_blocks(body)
    rendered = next(body.iter(f"{W}lastRenderedPageBreak"), None) is not None
    breaking = set() if rendered else styles_breaking_before(document)
    pages, page, pending = [], 1, 0
    for index, block in enumerate(blocks):
        page += pending
        pending = 0
        leading, trailing = break_positions(block, rendered)
        if index > 0 and (leading or (not rendered and breaks_before(block, breaking))):
            if pages and pages[-1] == page:
                page += 1
        pages.append(page)
        pending += trailing
        sectpr = own_sectpr(block)
        if not rendered and sectpr is not None and section_starts_new_page(sectpr):
            pending = max(pending, 1)
    return pages, "word-layout" if rendered else "explicit-breaks"


def referenced_ids(element) -> set[str]:
    found = set()
    for node in element.iter():
        for name, value in node.attrib.items():
            if name.startswith(R):
                found.add(value)
    return found


def prune_relationships(part) -> list[str]:
    used = referenced_ids(part.element)
    removed = []
    for identifier, relationship in list(part.rels.items()):
        if identifier in used or not relationship.reltype.endswith(PRUNABLE):
            continue
        target = relationship.target_ref
        del part.rels[identifier]
        removed.append(target)
    return removed


def parse_ranges(spec: str, count: int, base: int) -> list[int]:
    picked: list[int] = []
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, _, end = part.partition("-")
            first = int(start) if start else base
            last = int(end) if end else count - 1 + base
        else:
            first = last = int(part)
        picked.extend(range(first, last + 1))
    return list(dict.fromkeys(picked))


def format_ranges(numbers) -> str:
    ordered = sorted(set(numbers))
    parts, start = [], None
    for position, number in enumerate(ordered):
        if start is None:
            start = number
        if position + 1 == len(ordered) or ordered[position + 1] != number + 1:
            parts.append(str(start) if start == number else f"{start}-{number}")
            start = None
    return ",".join(parts)
