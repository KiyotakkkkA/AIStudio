# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
import argparse
import json
import re
import sys
from pathlib import Path

KEY = re.compile(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$")
VERSION = re.compile(r"^v?\d+(\.\d+){0,2}([-+].*)?$")
FORBIDDEN = re.compile(r'[\x00-\x1f\\:*?"<>|]')
DIRS = ("references", "scripts", "assets")
KEY_MAX = 64
NAME_MAX = 120
DESCRIPTION_MAX = 1024
INSTRUCTIONS_MAX = 100_000
FILES_MAX = 200
FILE_MAX = 1024 * 1024
TOTAL_MAX = 10 * 1024 * 1024
PATH_MAX = 240


def unquote(value: str) -> str:
    value = value.strip()
    if value.startswith('"'):
        return json.loads(value)
    if value.startswith("'") and value.endswith("'"):
        return value[1:-1].replace("''", "'")
    return value.split(" #", 1)[0].rstrip()


def parse(text: str) -> tuple[dict, dict, str, list[str]]:
    problems: list[str] = []
    lines = text.lstrip("﻿").splitlines()
    if not lines or lines[0].rstrip() != "---":
        return {}, {}, "", ["SKILL.md must start with a --- line"]
    try:
        close = next(i for i in range(1, len(lines)) if lines[i].rstrip() == "---")
    except StopIteration:
        return {}, {}, "", ["the frontmatter has no closing --- line"]
    fields: dict = {}
    metadata: dict = {}
    section = None
    for number, line in enumerate(lines[1:close], start=2):
        if not line.strip() or line.strip().startswith("#"):
            continue
        nested = line[:1] in (" ", "\t")
        if ":" not in line:
            problems.append(f"line {number}: expected key: value")
            continue
        key, value = line.split(":", 1)
        key = key.strip()
        if nested:
            if section == "metadata":
                metadata[key] = unquote(value)
            elif section == "allowed-tools" and key.startswith("-"):
                fields.setdefault("allowed-tools", []).append(unquote(key[1:] + ":" + value))
            elif section != "allowed-tools":
                problems.append(f"line {number}: unexpected indentation")
            continue
        if value.strip() in ("|", ">", "|-", ">-", "|+", ">+"):
            problems.append(f"line {number}: multi-line values are not supported")
            continue
        section = key if not value.strip() else None
        if section is None:
            fields[key] = unquote(value)
    instructions = "\n".join(lines[close + 1 :]).strip()
    return fields, metadata, instructions, problems


def check(folder: Path) -> dict:
    problems: list[str] = []
    warnings: list[str] = []
    manifest = folder / "SKILL.md"
    if not manifest.is_file():
        return {"ok": False, "problems": ["SKILL.md is missing"], "warnings": []}
    fields, metadata, instructions, parse_problems = parse(manifest.read_text(encoding="utf-8"))
    problems.extend(parse_problems)

    name = fields.get("name", "")
    if not KEY.match(name) or len(name) > KEY_MAX:
        problems.append("name must be lowercase letters, digits and single hyphens, at most 64 characters")
    if name and name != folder.name:
        problems.append(f"name '{name}' differs from the folder name '{folder.name}'")
    description = fields.get("description", "")
    if not description.strip():
        problems.append("description is required")
    elif len(description) > DESCRIPTION_MAX:
        problems.append(f"description is {len(description)} characters, the limit is {DESCRIPTION_MAX}")
    elif "Используется" not in description and "use" not in description.lower():
        warnings.append("description does not say when the skill is used")
    display = metadata.get("display-name", "")
    if len(display) > NAME_MAX:
        problems.append(f"display-name is longer than {NAME_MAX} characters")
    version = metadata.get("version")
    if version is None:
        warnings.append("metadata.version is missing; use 1.0.0")
    elif not VERSION.match(version):
        problems.append(f"version '{version}' is not major.minor.patch")
    if not instructions:
        problems.append("the instructions after the frontmatter are empty")
    elif len(instructions) > INSTRUCTIONS_MAX:
        problems.append("the instructions are longer than 100000 characters")
    elif instructions.count("\n") > 400:
        warnings.append("the instructions are long; move details to references/")

    files = []
    total = 0
    for path in sorted(folder.rglob("*")):
        if path.is_dir() or "__pycache__" in path.parts:
            continue
        relative = path.relative_to(folder).as_posix()
        if relative == "SKILL.md":
            continue
        segments = relative.split("/")
        if segments[0] not in DIRS or len(segments) < 2:
            problems.append(f"{relative}: files belong under references/, scripts/ or assets/")
        elif len(relative) > PATH_MAX or any(
            not part or part.startswith(".") or part.endswith((".", " ")) or FORBIDDEN.search(part)
            for part in segments
        ):
            problems.append(f"{relative}: the path contains a forbidden name or character")
        size = path.stat().st_size
        total += size
        if size > FILE_MAX:
            problems.append(f"{relative}: larger than 1 MiB")
        if relative.startswith("scripts/") and relative.endswith(".py"):
            head = path.read_text(encoding="utf-8", errors="replace")[:600]
            if "# /// script" not in head:
                warnings.append(f"{relative}: no '# /// script' header with dependencies")
        if relative not in instructions:
            warnings.append(f"{relative}: not mentioned in the instructions")
        files.append(relative)
    if len(files) > FILES_MAX:
        problems.append(f"{len(files)} files, the limit is {FILES_MAX}")
    if total > TOTAL_MAX:
        problems.append("the files exceed 10 MiB in total")
    for match in re.findall(r"`((?:references|scripts|assets)/[^`\s]+)`", instructions):
        if match not in files and "…" not in match and "*" not in match:
            warnings.append(f"the instructions mention {match}, which is not in this skill")

    return {
        "ok": not problems,
        "name": name,
        "version": version,
        "files": len(files),
        "problems": problems,
        "warnings": warnings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Check a skill folder before saving or importing it.")
    parser.add_argument("folder", help="absolute path of the skill folder")
    args = parser.parse_args()
    folder = Path(args.folder)
    if not folder.is_dir():
        print(f"not a folder: {folder}", file=sys.stderr)
        return 2
    report = check(folder)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
