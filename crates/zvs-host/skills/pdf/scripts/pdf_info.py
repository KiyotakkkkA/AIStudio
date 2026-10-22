# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf==6.21.2"]
# ///
import argparse
import json
import sys

from pypdf import PdfReader


def text_layer(reader: PdfReader, sample: int) -> bool:
    for page in reader.pages[:sample]:
        try:
            if (page.extract_text() or "").strip():
                return True
        except Exception:
            continue
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description="Print PDF facts as JSON.")
    parser.add_argument("file")
    parser.add_argument("--password", default=None)
    args = parser.parse_args()

    reader = PdfReader(args.file)
    encrypted = reader.is_encrypted
    if encrypted:
        if args.password is None or not reader.decrypt(args.password):
            print(json.dumps({"encrypted": True, "unlocked": False}, ensure_ascii=False))
            return 0

    sizes: dict[str, int] = {}
    for page in reader.pages:
        box = page.mediabox
        width = round(float(box.width) * 25.4 / 72)
        height = round(float(box.height) * 25.4 / 72)
        rotation = page.rotation % 360
        key = f"{width}x{height} mm" + (f" rotated {rotation}" if rotation else "")
        sizes[key] = sizes.get(key, 0) + 1

    meta = reader.metadata or {}
    fields = reader.get_fields() or {}
    info = {
        "pages": len(reader.pages),
        "encrypted": encrypted,
        "pdfVersion": reader.pdf_header.replace("%PDF-", ""),
        "title": meta.get("/Title"),
        "author": meta.get("/Author"),
        "subject": meta.get("/Subject"),
        "creator": meta.get("/Creator"),
        "producer": meta.get("/Producer"),
        "created": str(meta.get("/CreationDate")) if meta.get("/CreationDate") else None,
        "pageSizes": sizes,
        "formFields": len(fields),
        "outline": len(reader.outline) if reader.outline else 0,
        "textLayer": text_layer(reader, 5),
    }
    print(json.dumps(info, ensure_ascii=False, indent=2, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
