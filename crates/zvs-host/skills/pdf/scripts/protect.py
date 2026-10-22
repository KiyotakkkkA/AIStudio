# /// script
# requires-python = ">=3.10"
# dependencies = ["pypdf[crypto]==6.21.2"]
# ///
import argparse
import sys
from pathlib import Path

from pypdf import PdfReader, PdfWriter


def main() -> int:
    parser = argparse.ArgumentParser(description="Encrypt or decrypt a PDF.")
    parser.add_argument("action", choices=["encrypt", "decrypt"])
    parser.add_argument("file")
    parser.add_argument("output")
    parser.add_argument("--password", required=True)
    parser.add_argument("--owner-password", default=None)
    args = parser.parse_args()

    reader = PdfReader(args.file)
    if reader.is_encrypted and not reader.decrypt(args.password):
        print("The password does not open this document.", file=sys.stderr)
        return 2
    if args.action == "decrypt" and not reader.is_encrypted:
        print("The document is not encrypted.", file=sys.stderr)
        return 2

    writer = PdfWriter(clone_from=reader)
    if args.action == "encrypt":
        writer.encrypt(
            user_password=args.password,
            owner_password=args.owner_password or args.password,
            algorithm="AES-256",
        )
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as handle:
        writer.write(handle)
    print(f"{args.action.capitalize()}ed {len(reader.pages)} pages -> {output.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
