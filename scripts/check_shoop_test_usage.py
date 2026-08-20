#!/usr/bin/env python3
"""Require project Rust tests to use the shared test attribute."""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUST_ROOT = ROOT / "src" / "rust"
MACRO_IMPLEMENTATION = RUST_ROOT / "shoop_test_macros" / "src" / "lib.rs"
FORBIDDEN = re.compile(
    r"#\[\s*(?:::)?(?:(?:[A-Za-z_][A-Za-z0-9_]*::)*)"
    r"(?:test|wasm_bindgen_test|tracy_capture_test)\b[^]]*\]"
    r"|#\[\s*cfg_attr\([^]]*,\s*(?:::)?"
    r"(?:(?:[A-Za-z_][A-Za-z0-9_]*::)*)"
    r"(?:test|wasm_bindgen_test|tracy_capture_test)\b[^]]*\]"
)
GENERATED_ATTRIBUTES = {
    "#[test]",
    "#[::shoop_wasm_test_support::tracy_capture_test]",
    "#[::shoop_wasm_test_support::wasm_bindgen_test]",
}


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def main() -> int:
    errors: list[str] = []
    for path in sorted(RUST_ROOT.glob("*/**/*.rs")):
        text = path.read_text(errors="replace")
        for match in FORBIDDEN.finditer(text):
            attribute = match.group(0)
            if path == MACRO_IMPLEMENTATION and attribute in GENERATED_ATTRIBUTES:
                following = text[match.end() :].lstrip()
                if following.startswith(("#native", "#function")):
                    continue
            relative = path.relative_to(ROOT)
            errors.append(
                f"{relative}:{line_number(text, match.start())}: "
                f"use #[shoop_wasm_test_support::shoop_test] instead of {attribute}"
            )
    if errors:
        print("Shoop test attribute policy failed:", file=sys.stderr)
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Shoop test attribute policy: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
