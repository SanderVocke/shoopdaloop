#!/usr/bin/env python3
"""Require an explicit parity disposition for every application capability."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def violations(api: str, ledger: str) -> list[str]:
    block = re.search(r"pub enum BackendCapability\s*\{(.*?)\n\}", api, re.S)
    if block is None:
        return ["BackendCapability definition was not found"]
    capabilities = set(re.findall(r"^\s*([A-Za-z][A-Za-z0-9]*)\s*,", block[1], re.M))
    rows = re.findall(r"^\| `([A-Za-z][A-Za-z0-9]*)` \| (P\d{2}|AVAILABLE) \|$", ledger, re.M)
    mapped = {name for name, _ in rows}
    failures = [f"untracked capability: {name}" for name in sorted(capabilities - mapped)]
    failures += [f"stale capability: {name}" for name in sorted(mapped - capabilities)]
    if len(rows) != len(mapped):
        failures.append("duplicate capability dispositions")
    for name, item in rows:
        if item != "AVAILABLE" and not re.search(rf"^\| {item} (OPEN|COMPLETE) \|", ledger, re.M):
            failures.append(f"{name} points to a missing parity item: {item}")
    return failures


def main() -> int:
    failures = violations(
        (ROOT / "src/rust/shoop_app_api/src/lib.rs").read_text(),
        (ROOT / "docs/native_session_recovery/feature-parity.md").read_text(),
    )
    if failures:
        print("\n".join(failures))
        return 1
    print("Feature parity capability coverage: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
