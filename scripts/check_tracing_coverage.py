#!/usr/bin/env python3
"""Verify that the Rust tracing coverage inventory accounts for every module."""

from __future__ import annotations

import argparse
import csv
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs" / "tracing_coverage.csv"
ALLOWED = {
    "planned_direct",
    "planned_indirect",
    "instrumented_direct",
    "instrumented_indirect",
    "excluded",
}


def source_modules() -> set[str]:
    modules: set[str] = set()
    for path in (ROOT / "src" / "rust").rglob("*.rs"):
        relative = path.relative_to(ROOT)
        # Cargo integration tests are validation code, not production modules.
        if "tests" in relative.parts:
            continue
        modules.add(relative.as_posix())
    return modules


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--require-closed",
        action="store_true",
        help="reject planned entries; use for the final coverage audit",
    )
    args = parser.parse_args()

    errors: list[str] = []
    rows: dict[str, dict[str, str]] = {}
    with INVENTORY.open(newline="", encoding="utf-8") as inventory_file:
        reader = csv.DictReader(inventory_file)
        expected_fields = ["path", "context", "classification", "coverage_or_rationale"]
        if reader.fieldnames != expected_fields:
            errors.append(
                f"unexpected columns {reader.fieldnames!r}; expected {expected_fields!r}"
            )
        for line, row in enumerate(reader, start=2):
            path = row.get("path", "")
            classification = row.get("classification", "")
            if not path:
                errors.append(f"line {line}: empty path")
                continue
            if path in rows:
                errors.append(f"line {line}: duplicate path {path}")
            rows[path] = row
            if classification not in ALLOWED:
                errors.append(
                    f"line {line}: invalid classification {classification!r} for {path}"
                )
            if not row.get("context"):
                errors.append(f"line {line}: empty context for {path}")
            if not row.get("coverage_or_rationale"):
                errors.append(f"line {line}: empty coverage/rationale for {path}")
            if args.require_closed and classification.startswith("planned_"):
                errors.append(f"line {line}: unresolved planned coverage for {path}")

    sources = source_modules()
    inventoried = set(rows)
    for path in sorted(sources - inventoried):
        errors.append(f"missing source module: {path}")
    for path in sorted(inventoried - sources):
        errors.append(f"inventory path is not a production source module: {path}")

    if errors:
        print("Tracing coverage inventory failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    classifications: dict[str, int] = {}
    for row in rows.values():
        key = row["classification"]
        classifications[key] = classifications.get(key, 0) + 1
    summary = ", ".join(
        f"{key}={classifications[key]}" for key in sorted(classifications)
    )
    print(f"Tracing coverage inventory: {len(rows)} modules ({summary})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
