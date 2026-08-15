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
ROBOTO_FONT_FILES = (
    "Roboto-Regular.ttf",
    "Roboto-Italic.ttf",
    "Roboto-Bold.ttf",
    "Roboto-BoldItalic.ttf",
)
ICON_FILE = "icon.png"


def exactly_one(paths: list[Path], description: str) -> Path:
    if len(paths) != 1:
        names = ", ".join(path.name for path in paths) or "none"
        raise RuntimeError(f"expected one {description}, found: {names}")
    return paths[0]


def build_single_file(dist: Path, output: Path) -> None:
    index_path = dist / "index.html"
    html = index_path.read_text(encoding="utf-8")
    js_path = exactly_one(
        list(dist.glob("shoopdaloop-*.js")), "wasm-bindgen JavaScript file"
    )
    wasm_path = exactly_one(
        list(dist.glob("shoopdaloop-*_bg.wasm")), "WebAssembly file"
    )
    raw_host_script_path = dist / "raw_wasm_host.js"
    worklet_script_path = dist / "audio_worklet.js"
    worker_script_path = dist / "audio_worker.js"
    worklet_wasm_path = dist / "generated" / "shoop_audio_worklet.wasm"
    for path, description in [
        (raw_host_script_path, "raw Wasm host bridge"),
        (worklet_script_path, "AudioWorklet script"),
        (worker_script_path, "Worker engine script"),
        (worklet_wasm_path, "AudioWorklet WebAssembly file"),
    ]:
        if not path.is_file():
            raise RuntimeError(f"missing {description}: {path}")

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

    icon_path = dist / ICON_FILE
    if not icon_path.is_file():
        raise RuntimeError(f"missing application icon: {icon_path}")
    icon_url = f'href="./{ICON_FILE}"'
    if html.count(icon_url) != 1:
        raise RuntimeError("could not identify application icon URL")
    encoded_icon = base64.b64encode(icon_path.read_bytes()).decode("ascii")
    html = html.replace(icon_url, f'href="data:image/png;base64,{encoded_icon}"')

    for name in ROBOTO_FONT_FILES:
        font_path = dist / "roboto" / name
        if not font_path.is_file():
            raise RuntimeError(f"missing Roboto font: {font_path}")
        font_url = f'url("./roboto/{name}")'
        if html.count(font_url) != 1:
            raise RuntimeError(f"could not identify Roboto font URL for {name}")
        encoded_font = base64.b64encode(font_path.read_bytes()).decode("ascii")
        html = html.replace(font_url, f'url("data:font/ttf;base64,{encoded_font}")')

    glue = js_path.read_text(encoding="utf-8")
    if "</script>" in glue.lower():
        raise RuntimeError("wasm-bindgen glue contains an unsafe </script> sequence")
    glue, count = WASM_BINDGEN_EXPORT.subn("\n", glue)
    if count != 1:
        raise RuntimeError("wasm-bindgen export footer has an unexpected shape")

    raw_host_source = raw_host_script_path.read_text(encoding="utf-8")
    worklet_source = worklet_script_path.read_text(encoding="utf-8").replace(
        "import './raw_wasm_host.js';\n", ""
    )
    worker_source = worker_script_path.read_text(encoding="utf-8").replace(
        "importScripts('./raw_wasm_host.js');\n", ""
    )
    encoded_worklet_source = base64.b64encode(
        f"{raw_host_source}\n{worklet_source}".encode("utf-8")
    ).decode("ascii")
    encoded_worker_source = base64.b64encode(
        f"{raw_host_source}\n{worker_source}".encode("utf-8")
    ).decode("ascii")
    encoded_worklet_wasm = base64.b64encode(worklet_wasm_path.read_bytes()).decode(
        "ascii"
    )
    encoded_wasm = base64.b64encode(wasm_path.read_bytes()).decode("ascii")
    embedded_script = f"""
<script type="module">
const shoopAudioWorkletModuleUrl = "data:text/javascript;base64,{encoded_worklet_source}";
const shoopAudioWorkerUrl = "data:text/javascript;base64,{encoded_worker_source}";
const shoopAudioWorkletBinary = atob("{encoded_worklet_wasm}");
const shoopAudioWorkletWasmBytes = Uint8Array.from(
    shoopAudioWorkletBinary,
    character => character.charCodeAt(0),
);
globalThis.shoopEmbeddedAudioWorklet = Object.freeze({{
    moduleUrl: shoopAudioWorkletModuleUrl,
    workerUrl: shoopAudioWorkerUrl,
    wasmBytes: shoopAudioWorkletWasmBytes,
}});
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
        help="output file (default: DIST/shoopdaloop.html)",
    )
    args = parser.parse_args()
    output = args.output or args.dist / "shoopdaloop.html"
    build_single_file(args.dist, output)
    print(f"wrote self-contained application: {output} ({output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
