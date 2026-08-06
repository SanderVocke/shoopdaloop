#!/usr/bin/env python3
"""Embed a Trunk web build into one directly openable HTML file."""

from __future__ import annotations

import argparse
import base64
import re
from pathlib import Path

TRUNK_SCRIPT = re.compile(r'\s*<script type="module">(?P<body>.*?)</script>', re.DOTALL)
WASM_BINDGEN_EXPORT = re.compile(r'\nexport \{ initSync, __wbg_init as default \};\s*\Z')


def exactly_one(paths: list[Path], description: str) -> Path:
    if len(paths) != 1:
        names = ", ".join(path.name for path in paths) or "none"
        raise RuntimeError(f"expected one {description}, found: {names}")
    return paths[0]


def build_single_file(dist: Path, output: Path) -> None:
    html = (dist / "index.html").read_text(encoding="utf-8")
    js_path = exactly_one(list(dist.glob("*.js")), "wasm-bindgen JavaScript file")
    wasm_path = exactly_one(list(dist.glob("*.wasm")), "WebAssembly file")
    matches = [m for m in TRUNK_SCRIPT.finditer(html) if "TrunkApplicationStarted" in m.group("body")]
    if len(matches) != 1:
        raise RuntimeError("could not identify Trunk's module bootstrap script")
    bootstrap = matches[0]
    html = html[:bootstrap.start()] + html[bootstrap.end():]
    for asset in (js_path.name, wasm_path.name):
        pattern = re.compile(rf'<link\s+[^>]*href="\./{re.escape(asset)}"[^>]*>', re.DOTALL)
        html, count = pattern.subn("", html)
        if count != 1:
            raise RuntimeError(f"could not remove preload for {asset}")
    glue = js_path.read_text(encoding="utf-8")
    if "</script>" in glue.lower():
        raise RuntimeError("wasm-bindgen glue contains an unsafe </script> sequence")
    glue, count = WASM_BINDGEN_EXPORT.subn("\n", glue)
    if count != 1:
        raise RuntimeError("wasm-bindgen export footer has an unexpected shape")
    encoded = base64.b64encode(wasm_path.read_bytes()).decode("ascii")
    script = f'''\n<script type="module">\n{glue}\nconst shoopWasmBinary = atob("{encoded}");\nconst shoopWasmBytes = Uint8Array.from(shoopWasmBinary, c => c.charCodeAt(0));\nconst shoopWasm = await __wbg_init({{ module_or_path: shoopWasmBytes }});\nwindow.wasmBindings = Object.freeze({{ default: __wbg_init, initSync }});\ndispatchEvent(new CustomEvent("TrunkApplicationStarted", {{ detail: {{ wasm: shoopWasm }} }}));\n</script>\n'''
    html = html.replace("</head>", f"{script}</head>", 1)
    output.write_text(html, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("dist", nargs="?", type=Path, default=Path("dist"))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    output = args.output or args.dist / "preview.html"
    build_single_file(args.dist, output)
    print(f"wrote self-contained preview: {output} ({output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
