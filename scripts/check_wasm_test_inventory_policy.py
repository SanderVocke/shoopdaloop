#!/usr/bin/env python3
"""Fail when source or runtime test membership changes without policy review."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
import tomllib
import xml.etree.ElementTree as ET
from collections import Counter

from wasm_test_inventory import classify, source_tests, wasm_tests

ROOT = pathlib.Path(__file__).resolve().parents[1]


def digest(lines: list[str]) -> str:
    return hashlib.sha256(("\n".join(lines) + "\n").encode()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", action="append", type=pathlib.Path, default=[])
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument(
        "--classification",
        type=pathlib.Path,
        default=ROOT / "tests/wasm_test_classification.toml",
    )
    args = parser.parse_args()
    try:
        classification = tomllib.loads(args.classification.read_text())
        policy = classification["inventory"]
        declarations = source_tests()
        source_rows = [
            "\t".join(
                [item["package"], item["path"], item["name"], item["attribute"]]
            )
            for item in declarations
        ]
        actual_source_hash = digest(source_rows)
        if len(declarations) != policy["source_declaration_count"]:
            raise ValueError(
                f"source declaration count changed: {len(declarations)} != "
                f"{policy['source_declaration_count']}"
            )
        if actual_source_hash != policy["source_declarations_sha256"]:
            raise ValueError(
                "source declaration membership changed: "
                f"{actual_source_hash} != {policy['source_declarations_sha256']}"
            )

        runtime_sets: list[set[str]] = []
        runtime_reports = []
        for summary in args.summary:
            tests = wasm_tests(summary)
            identifiers = [test["id"] for test in tests]
            if len(identifiers) != policy["wasm_test_count"]:
                raise ValueError(
                    f"{summary} test count changed: {len(identifiers)} != "
                    f"{policy['wasm_test_count']}"
                )
            actual_runtime_hash = digest(identifiers)
            if actual_runtime_hash != policy["wasm_test_ids_sha256"]:
                raise ValueError(
                    f"{summary} membership changed: {actual_runtime_hash} != "
                    f"{policy['wasm_test_ids_sha256']}"
                )
            runtime_sets.append(set(identifiers))
            categories = Counter(
                classify(identifier, classification["rules"])["category"]
                for identifier in identifiers
            )
            document = json.loads(summary.read_text())
            for package in document["packages"]:
                log = ROOT / package["log"]
                junit = ROOT / package["junit"]
                if not log.is_file() or not junit.is_file():
                    raise ValueError(
                        f"{summary} package {package['package']} is missing raw log or JUnit"
                    )
                suite = ET.parse(junit).getroot()
                properties = {
                    item.attrib["name"]: item.attrib["value"]
                    for item in suite.find("properties")
                }
                required = {
                    "package",
                    "runtime",
                    "profile",
                    "command",
                    "returncode",
                    "expected",
                    "listed",
                    "executed",
                    "passed",
                    "failed",
                    "ignored",
                    "raw_log",
                    "filters",
                    "features",
                    "tool.node",
                    "tool.rustc",
                    "tool.wasm-pack",
                    "tool.wasm-bindgen",
                    "tool.wasm-bindgen-test",
                }
                missing = required - properties.keys()
                if missing:
                    raise ValueError(
                        f"{junit} is missing properties: {', '.join(sorted(missing))}"
                    )
                if int(suite.attrib["tests"]) != package["tests"]:
                    raise ValueError(f"{junit} testcase count differs from summary")
                if int(suite.attrib["failures"]) != package["failed"]:
                    raise ValueError(f"{junit} failure count differs from summary")
                if int(suite.attrib["skipped"]) != package["ignored"]:
                    raise ValueError(f"{junit} ignored count differs from summary")
                if properties["raw_log"] != package["log"]:
                    raise ValueError(f"{junit} raw-log reference differs from summary")
            runtime_reports.append(
                {
                    "runtime": document["runtime"],
                    "profile": document["profile"],
                    "count": len(identifiers),
                    "ids_sha256": actual_runtime_hash,
                    "category_counts": dict(sorted(categories.items())),
                    "elapsed_seconds": sum(
                        package["elapsed_seconds"] for package in document["packages"]
                    ),
                    "summary": str(summary),
                }
            )
        if runtime_sets and any(items != runtime_sets[0] for items in runtime_sets[1:]):
            raise ValueError("runtime test memberships differ")
        if args.output:
            report = {
                "schema": 1,
                "source_declaration_count": len(declarations),
                "source_declarations_sha256": actual_source_hash,
                "wasm_test_count": policy["wasm_test_count"],
                "wasm_test_ids_sha256": policy["wasm_test_ids_sha256"],
                "runtime_reports": runtime_reports,
            }
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        print(
            f"Wasm inventory policy: source={len(declarations)} "
            f"runtime={policy['wasm_test_count']} summaries={len(args.summary)}: ok"
        )
        return 0
    except (
        KeyError,
        OSError,
        ValueError,
        ET.ParseError,
        tomllib.TOMLDecodeError,
    ) as error:
        print(f"Wasm inventory policy failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
