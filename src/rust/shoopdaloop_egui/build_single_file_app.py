#!/usr/bin/env python3
"""Embed a Trunk web build into one directly openable HTML file."""

from __future__ import annotations

import argparse
import base64
import re
from pathlib import Path

TRUNK_SCRIPT = re.compile(
    r"\s*<script type=\"module\">(?P<body>.*?)</script>", re.DOTALL
)
WASM_BINDGEN_EXPORT = re.compile(
    r"\nexport \{ initSync, __wbg_init as default \};\s*\Z"
)


def exactly_one(paths: list[Path], description: str) -> Path:
    if len(paths) != 1:
        names = ", ".join(path.name for path in paths) or "none"
        raise RuntimeError(f"expected one {description}, found: {names}")
    return paths[0]


def build_single_file(dist: Path, output: Path) -> None:
    index_path = dist / "index.html"
    html = index_path.read_text(encoding="utf-8")
    js_path = exactly_one(list(dist.glob("*.js")), "wasm-bindgen JavaScript file")
    wasm_path = exactly_one(list(dist.glob("*.wasm")), "WebAssembly file")

    script_matches = [
        match
        for match in TRUNK_SCRIPT.finditer(html)
        if "TrunkApplicationStarted" in match.group("body")
    ]
    if len(script_matches) != 1:
        raise RuntimeError("could not identify Trunk's module bootstrap script")
    bootstrap = script_matches[0]
    html = html[: bootstrap.start()] + html[bootstrap.end() :]

    for asset in (js_path.name, wasm_path.name):
        preload = re.compile(
            rf"<link\s+[^>]*href=\"\./{re.escape(asset)}\"[^>]*>", re.DOTALL
        )
        html, count = preload.subn("", html)
        if count != 1:
            raise RuntimeError(f"could not remove preload for {asset}")

    glue = js_path.read_text(encoding="utf-8")
    if "</script>" in glue.lower():
        raise RuntimeError("wasm-bindgen glue contains an unsafe </script> sequence")
    glue, count = WASM_BINDGEN_EXPORT.subn("\n", glue)
    if count != 1:
        raise RuntimeError("wasm-bindgen export footer has an unexpected shape")

    encoded_wasm = base64.b64encode(wasm_path.read_bytes()).decode("ascii")
    embedded_script = f"""
<script type="module">
{glue}
const shoopWasmBinary = atob("{encoded_wasm}");
const shoopWasmBytes = Uint8Array.from(
    shoopWasmBinary,
    character => character.charCodeAt(0),
);
const shoopWasm = await __wbg_init({{ module_or_path: shoopWasmBytes }});
window.wasmBindings = Object.freeze({{ default: __wbg_init, initSync }});
dispatchEvent(new CustomEvent("TrunkApplicationStarted", {{
    detail: {{ wasm: shoopWasm }},
}}));
</script>
"""
    if "</head>" not in html:
        raise RuntimeError("index.html has no closing head element")
    html = html.replace("</head>", f"{embedded_script}</head>", 1)

    output.write_text(html, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "dist",
        nargs="?",
        type=Path,
        default=Path("dist"),
        help="Trunk output directory (default: dist)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="output file (default: DIST/shoopdaloop_egui.html)",
    )
    args = parser.parse_args()
    output = args.output or args.dist / "shoopdaloop_egui.html"
    build_single_file(args.dist, output)
    print(f"wrote self-contained application: {output} ({output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
