#!/usr/bin/env python3
"""Enforce the three-invocation packaged-browser smoke budget."""

from __future__ import annotations

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/build_and_test.yml"


def main() -> int:
    workflow = WORKFLOW.read_text()
    chrome = [line for line in workflow.splitlines() if "browser_smoke.mjs" in line]
    firefox = [
        line for line in workflow.splitlines() if "browser_firefox_smoke.py" in line
    ]
    errors = []
    if len(chrome) != 2:
        errors.append(f"expected two Chromium smoke invocations, found {len(chrome)}")
    if len(firefox) != 1:
        errors.append(f"expected one Firefox smoke invocation, found {len(firefox)}")
    if any("OUTPUT_ONLY=1" not in line for line in chrome):
        errors.append("every Chromium smoke must use the minimal OUTPUT_ONLY mode")
    if "SELF_CONTAINED=1 OUTPUT_ONLY=1" not in workflow:
        errors.append("self-contained Chromium smoke is missing")
    if not (ROOT / "docs/wasm_smoke_migration.md").is_file():
        errors.append("smoke migration map is missing")
    if errors:
        print("Browser smoke budget failed: " + "; ".join(errors), file=sys.stderr)
        return 1
    print("Browser smoke budget: 2 Chromium + 1 Firefox minimal invocations: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
