#!/usr/bin/env python3
"""Generate the hosted-browser catalog for the distributable built-ins tree."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath

EXTENSIONS = {
    ".lua": "lua",
    ".md": "markdown",
    ".markdown": "markdown",
    ".png": "image",
}


def generate(root: Path) -> dict:
    files = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.name == "catalog.json":
            continue
        relative = PurePosixPath(path.relative_to(root)).as_posix()
        kind = EXTENSIONS.get(path.suffix.lower())
        if kind is None:
            continue
        payload = path.read_bytes()
        files.append(
            {
                "path": relative,
                "kind": kind,
                "bytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
            }
        )
    return {"format": "shoop-builtins-catalog", "version": 1, "files": files}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "root", type=Path, nargs="?", default=Path("resources/builtins")
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    output = root / "catalog.json"
    encoded = json.dumps(generate(root), indent=2, sort_keys=True) + "\n"
    if args.check:
        if not output.is_file() or output.read_text(encoding="utf-8") != encoded:
            raise SystemExit(f"generated built-ins catalog is stale: {output}")
    else:
        output.write_text(encoded, encoding="utf-8")


if __name__ == "__main__":
    main()
