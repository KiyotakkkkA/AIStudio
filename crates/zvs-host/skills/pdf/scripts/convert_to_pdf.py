# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2", "pywin32==312; sys_platform == 'win32'"]
# ///
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from pypdf import PdfReader

WORD = (".docx", ".docm", ".doc", ".dotx", ".rtf", ".odt")
EXCEL = (".xlsx", ".xlsm", ".xls", ".ods", ".csv")
POWERPOINT = (".pptx", ".pptm", ".ppt", ".odp")


def with_office(source: Path, target: Path, update_fields: bool) -> None:
    import pythoncom
    from win32com.client import DispatchEx

    pythoncom.CoInitialize()
    suffix = source.suffix.lower()
    try:
        if suffix in WORD:
            app = DispatchEx("Word.Application")
            app.Visible = False
            app.DisplayAlerts = 0
            try:
                document = app.Documents.Open(str(source), False, True, False)
                try:
                    if update_fields:
                        for table in document.TablesOfContents:
                            table.Update()
                        document.Fields.Update()
                    document.ExportAsFixedFormat(str(target), 17, False, 0, 0, 1, 1, 0, True, True, 1)
                finally:
                    document.Close(0)
            finally:
                app.Quit()
        elif suffix in EXCEL:
            app = DispatchEx("Excel.Application")
            app.Visible = False
            app.DisplayAlerts = False
            try:
                workbook = app.Workbooks.Open(str(source), 0, True)
                try:
                    workbook.ExportAsFixedFormat(0, str(target))
                finally:
                    workbook.Close(False)
            finally:
                app.Quit()
        else:
            app = DispatchEx("PowerPoint.Application")
            try:
                presentation = app.Presentations.Open(str(source), True, False, False)
                try:
                    presentation.SaveAs(str(target), 32)
                finally:
                    presentation.Close()
            finally:
                app.Quit()
    finally:
        pythoncom.CoUninitialize()


def find_soffice() -> str | None:
    found = shutil.which("soffice") or shutil.which("libreoffice")
    if found:
        return found
    for root in (os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)"), "/Applications/LibreOffice.app/Contents/MacOS"):
        if not root:
            continue
        for candidate in (Path(root) / "LibreOffice" / "program" / "soffice.exe", Path(root) / "soffice"):
            if candidate.is_file():
                return str(candidate)
    return None


def with_libreoffice(soffice: str, source: Path, target: Path) -> None:
    with tempfile.TemporaryDirectory() as folder:
        profile = Path(folder) / "profile"
        subprocess.run(
            [soffice, f"-env:UserInstallation={profile.as_uri()}", "--headless", "--convert-to", "pdf", "--outdir", folder, str(source)],
            check=True,
            capture_output=True,
            timeout=100,
        )
        produced = Path(folder) / f"{source.stem}.pdf"
        if not produced.is_file():
            raise RuntimeError("LibreOffice produced no PDF")
        shutil.move(str(produced), target)


def main() -> int:
    parser = argparse.ArgumentParser(description="Convert a Word, Excel or PowerPoint file to PDF with Microsoft Office or LibreOffice.")
    parser.add_argument("file")
    parser.add_argument("output")
    parser.add_argument("--engine", choices=("auto", "office", "libreoffice"), default="auto")
    parser.add_argument("--no-update-fields", action="store_true", help="keep the table of contents and page fields as saved")
    args = parser.parse_args()

    source, target = Path(args.file).resolve(), Path(args.output).resolve()
    if not source.is_file():
        print(f"{source} does not exist.", file=sys.stderr)
        return 2
    if source.suffix.lower() not in WORD + EXCEL + POWERPOINT:
        print(f"{source.suffix} is not a Word, Excel or PowerPoint file.", file=sys.stderr)
        return 2
    target.parent.mkdir(parents=True, exist_ok=True)

    errors = []
    engine = None
    if args.engine in ("auto", "office") and sys.platform == "win32":
        try:
            with_office(source, target, not args.no_update_fields)
            engine = "microsoft-office"
        except Exception as error:
            errors.append(f"Microsoft Office: {error}")
    if engine is None and args.engine in ("auto", "libreoffice"):
        soffice = find_soffice()
        if soffice is None:
            errors.append("LibreOffice: not installed")
        else:
            try:
                with_libreoffice(soffice, source, target)
                engine = "libreoffice"
            except Exception as error:
                errors.append(f"LibreOffice: {error}")
    if engine is None:
        print("No converter worked. Microsoft Office or LibreOffice must be installed.\n" + "\n".join(errors), file=sys.stderr)
        return 2

    pages = len(PdfReader(str(target)).pages)
    print(json.dumps({"output": str(target), "engine": engine, "pages": pages}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
