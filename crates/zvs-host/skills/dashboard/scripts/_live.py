import datetime as dt
import hashlib
import json
import os
import shutil
from pathlib import Path

from _sources import split_source

LIVE_DIR = ".zvs"
CHAT_ENV = "ZVS_CHAT_DIR"
INTERVAL_MIN = 15
INTERVAL_MAX = 86_400
SCRIPT_SUFFIXES = {".py", ".js", ".mjs", ".cjs"}


def live_dir(library: Path, stem: str) -> Path:
    return library / LIVE_DIR / stem


def same(a: Path, b: Path) -> bool:
    return os.path.normcase(os.path.abspath(a)) == os.path.normcase(os.path.abspath(b))


def inside(path: Path, folder: Path) -> bool:
    try:
        return os.path.commonpath([os.path.normcase(os.path.abspath(path)), os.path.normcase(os.path.abspath(folder))]) == os.path.normcase(os.path.abspath(folder))
    except ValueError:
        return False


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_refresh(spec: dict, problems) -> None:
    refresh = spec.get("refresh")
    if refresh is None:
        return
    if not isinstance(refresh, dict) or set(refresh) - {"interval"}:
        problems.error('refresh: an object such as {"interval": 300}')
        return
    interval = refresh.get("interval", 0)
    if not isinstance(interval, int) or isinstance(interval, bool) or (interval and not INTERVAL_MIN <= interval <= INTERVAL_MAX):
        problems.error(f"refresh.interval: 0 (only on request) or seconds from {INTERVAL_MIN} to {INTERVAL_MAX}")


def check_fetch(spec: dict, base: Path, problems) -> list[dict]:
    steps = spec.get("fetch")
    if steps is None:
        return []
    if not isinstance(steps, list):
        problems.error('fetch: a list of {"script": "ABSOLUTE_PATH", "output": "ABSOLUTE_PATH"}')
        return []
    found = []
    for index, step in enumerate(steps):
        path = f"fetch[{index}]"
        if not isinstance(step, dict) or set(step) != {"script", "output"}:
            problems.error(f'{path}: exactly "script" and "output"')
            continue
        script = Path(str(step["script"]))
        output = Path(str(step["output"]))
        script = script if script.is_absolute() else base / script
        output = output if output.is_absolute() else base / output
        if script.suffix.lower() not in SCRIPT_SUFFIXES:
            problems.error(f"{path}.script: a .py or .js file")
            continue
        if not script.is_file():
            problems.error(f"{path}.script: file not found: {script}")
            continue
        if not output.is_file():
            problems.error(
                f"{path}.output: file not found: {output}; run the script once with shell.run, passing this path as its last argument, before building"
            )
            continue
        found.append({"script": script, "output": output})
    return found


def dataset_paths(spec: dict):
    for name, dataset in (spec.get("datasets") or {}).items():
        if not isinstance(dataset, dict):
            continue
        if isinstance(dataset.get("path"), str):
            yield name, dataset, "path"
        sources = dataset.get("sources")
        if isinstance(sources, dict):
            for key, value in sources.items():
                if isinstance(value, str):
                    yield name, sources, key


def absolute(text: str, base: Path) -> tuple[Path, str | None]:
    path_text, part = split_source(text)
    path = Path(path_text)
    return (path if path.is_absolute() else base / path), part


def with_part(path: Path, part: str | None) -> str:
    return f"{path.as_posix()}#{part}" if part else path.as_posix()


def unique(name: str, taken: set) -> str:
    stem, suffix = os.path.splitext(name)
    candidate, counter = name, 2
    while candidate.lower() in taken:
        candidate = f"{stem}-{counter}{suffix}"
        counter += 1
    taken.add(candidate.lower())
    return candidate


def stage(spec: dict, base: Path, fetch: list[dict], work: Path, final: Path, problems) -> dict:
    chat = os.environ.get(CHAT_ENV)
    chat_dir = Path(chat) if chat else None
    live = json.loads(json.dumps(spec, ensure_ascii=False))
    live.pop("fetch", None)
    (work / "fetch").mkdir(parents=True, exist_ok=True)
    (work / "files").mkdir(parents=True, exist_ok=True)
    taken: set = set()
    steps = []
    outputs = []
    for index, step in enumerate(fetch, start=1):
        script_name = unique(f"{index}-{step['script'].name}", taken)
        output_name = unique(f"{index}-{step['output'].name}", taken)
        shutil.copy2(step["script"], work / "fetch" / script_name)
        shutil.copy2(step["output"], work / "fetch" / output_name)
        steps.append(
            {
                "script": f"fetch/{script_name}",
                "output": f"fetch/{output_name}",
                "sha256": sha256(work / "fetch" / script_name),
            }
        )
        outputs.append((step["output"], output_name))
    used_outputs = set()
    file_names: set = set()
    copied: dict = {}
    refreshing = bool((spec.get("refresh") or {}).get("interval")) or bool(fetch)
    for name, owner, key in dataset_paths(live):
        path, part = absolute(owner[key], base)
        target = None
        for source, output_name in outputs:
            if same(path, source):
                target = Path("fetch") / output_name
                used_outputs.add(output_name)
                break
        if target is None and chat_dir is not None and inside(path, chat_dir) and path.is_file():
            if str(path) not in copied:
                copied[str(path)] = unique(path.name, file_names)
                shutil.copy2(path, work / "files" / copied[str(path)])
            target = Path("files") / copied[str(path)]
            if refreshing:
                problems.warn(
                    f"datasets.{name}: reads the working file {path.name}; refreshing will not change it. Read the original source with sql, or produce the file with a fetch script, to keep it live"
                )
        owner[key] = with_part(final / target if target is not None else path, part)
    for source, output_name in outputs:
        if output_name not in used_outputs:
            problems.warn(f"fetch: no dataset reads {source}; give that path in a dataset's path or sources")
    return {"spec": live, "fetch": steps}


def write_live(library: Path, stem: str, page: str, staged_dir: Path, staged: dict, data_json: str) -> None:
    target = live_dir(library, stem)
    meta = {
        "version": 1,
        "page": page,
        "interval": int((staged["spec"].get("refresh") or {}).get("interval") or 0),
        "fetch": staged["fetch"],
        "builtAt": dt.datetime.now().astimezone().isoformat(timespec="seconds"),
    }
    (staged_dir / "spec.json").write_text(json.dumps(staged["spec"], ensure_ascii=False, indent=1), encoding="utf-8")
    (staged_dir / "meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
    (staged_dir / "data.json").write_text(data_json, encoding="utf-8")
    if target.exists():
        shutil.rmtree(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(staged_dir), str(target))


def read_live(library: Path, stem: str) -> tuple[dict, dict]:
    folder = live_dir(library, stem)
    spec = json.loads((folder / "spec.json").read_text(encoding="utf-8"))
    meta = json.loads((folder / "meta.json").read_text(encoding="utf-8"))
    return spec, meta


def refresh_live(library: Path, stem: str, meta: dict, data_json: str) -> None:
    folder = live_dir(library, stem)
    meta["builtAt"] = dt.datetime.now().astimezone().isoformat(timespec="seconds")
    temporary = folder / "data.json.tmp"
    temporary.write_text(data_json, encoding="utf-8")
    os.replace(temporary, folder / "data.json")
    (folder / "meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
