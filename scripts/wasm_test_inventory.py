#!/usr/bin/env python3

from __future__ import annotations

import argparse
import fnmatch
import json
import pathlib
import re
import sys
import tomllib
from collections import Counter

ROOT = pathlib.Path(__file__).resolve().parents[1]
ALLOWED = {
    "shared",
    "native-driver",
    "native-platform",
    "wasm-runtime",
    "browser-adapter",
    "packaged-smoke",
    "pending",
}
TEST_ATTRIBUTES = {
    "test",
    "tracy_nextest_capture::tracy_capture_test",
    "shoop_wasm_test_support::shoop_test",
    "shoop_test",
    "wasm_bindgen_test",
    "wasm_bindgen_test::wasm_bindgen_test",
}


def native_tests(path: pathlib.Path):
    document = json.loads(path.read_text())
    tests = []
    for suite in document["rust-suites"].values():
        package = suite["package-name"]
        binary = suite["binary-id"]
        for name, details in suite["testcases"].items():
            tests.append(
                {
                    "id": f"{package}::{binary}::{name}",
                    "package": package,
                    "binary": binary,
                    "name": name,
                    "ignored": details["ignored"],
                }
            )
    tests.sort(key=lambda test: test["id"])
    if len(tests) != document["test-count"]:
        raise ValueError("native nextest count does not match listed testcases")
    return tests


def wasm_tests(path: pathlib.Path):
    document = json.loads(path.read_text())
    tests = []
    for package in document["packages"]:
        for case in package.get("testcases", []):
            tests.append(
                {
                    "id": f"{package['package']}::{case['name']}",
                    "package": package["package"],
                    "name": case["name"],
                    "status": case["status"],
                }
            )
    tests.sort(key=lambda test: test["id"])
    return tests


def source_tests():
    declarations = []
    for manifest in sorted((ROOT / "src/rust").glob("*/Cargo.toml")):
        package = tomllib.loads(manifest.read_text())["package"]["name"]
        for path in sorted(manifest.parent.rglob("*.rs")):
            pending: list[tuple[int, str]] = []
            lines = path.read_text(errors="replace").splitlines()
            index = 0
            while index < len(lines):
                stripped = lines[index].strip()
                if stripped.startswith("#["):
                    start = index
                    attribute = stripped
                    balance = stripped.count("[") - stripped.count("]")
                    while balance > 0 and index + 1 < len(lines):
                        index += 1
                        part = lines[index].strip()
                        attribute += " " + part
                        balance += part.count("[") - part.count("]")
                    pending.append((start + 1, attribute))
                elif stripped.startswith(("fn ", "async fn ", "pub fn ", "pub async fn ")):
                    recognized = []
                    for line, attribute in pending:
                        body = attribute[2:-1].strip().split("(", 1)[0]
                        if body in TEST_ATTRIBUTES:
                            recognized.append((line, body))
                    if recognized:
                        match = re.search(r"\bfn\s+([A-Za-z0-9_]+)", stripped)
                        if match:
                            line, attribute = recognized[-1]
                            declarations.append(
                                {
                                    "package": package,
                                    "path": str(path.relative_to(ROOT)),
                                    "line": line,
                                    "name": match.group(1),
                                    "attribute": attribute,
                                }
                            )
                    pending.clear()
                elif stripped and not stripped.startswith(("//", "///", "//!")):
                    pending.clear()
                index += 1
    return declarations


def classify(identifier: str, rules: list[dict]):
    matches = [
        rule
        for rule in rules
        if fnmatch.fnmatchcase(identifier, rule["pattern"])
        and not any(fnmatch.fnmatchcase(identifier, exclude) for exclude in rule.get("exclude", []))
    ]
    if not matches:
        raise ValueError(f"unclassified test: {identifier}")
    if len(matches) != 1:
        raise ValueError(f"overlapping classification rules for {identifier}: {matches}")
    return matches[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--native-json", type=pathlib.Path, required=True)
    parser.add_argument("--node-summary", type=pathlib.Path, required=True)
    parser.add_argument("--chrome-summary", type=pathlib.Path, required=True)
    parser.add_argument(
        "--classification",
        type=pathlib.Path,
        default=ROOT / "tests/wasm_test_classification.toml",
    )
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--require-closed", action="store_true")
    args = parser.parse_args()

    try:
        native = native_tests(args.native_json)
        node = wasm_tests(args.node_summary)
        chrome = wasm_tests(args.chrome_summary)
        declarations = source_tests()
        classification = tomllib.loads(args.classification.read_text())
        if classification.get("schema") != 1:
            raise ValueError("unsupported classification schema")
        rules = classification["rules"]
        for rule in rules:
            if rule["category"] not in ALLOWED:
                raise ValueError(f"invalid category in {rule}")
            if not rule.get("reason", "").strip():
                raise ValueError(f"classification reason is empty in {rule}")

        classified = []
        used_rules = Counter()
        for test in native:
            rule = classify(test["id"], rules)
            used_rules[rule["pattern"]] += 1
            classified.append({**test, "category": rule["category"], "reason": rule["reason"]})
        classified_node = []
        for test in node:
            rule = classify(test["id"], rules)
            used_rules[rule["pattern"]] += 1
            classified_node.append({**test, "category": rule["category"], "reason": rule["reason"]})
        classified_chrome = []
        for test in chrome:
            rule = classify(test["id"], rules)
            used_rules[rule["pattern"]] += 1
            classified_chrome.append({**test, "category": rule["category"], "reason": rule["reason"]})
        stale = sorted(rule["pattern"] for rule in rules if not used_rules[rule["pattern"]])
        if stale:
            raise ValueError("stale classification rules: " + ", ".join(stale))

        node_ids = {test["id"] for test in node}
        chrome_ids = {test["id"] for test in chrome}
        if node_ids != chrome_ids:
            raise ValueError(
                f"Node/Chromium inventory differs: node-only={sorted(node_ids-chrome_ids)}, "
                f"chrome-only={sorted(chrome_ids-node_ids)}"
            )
        shared_native = {(test["package"], test["name"]) for test in classified if test["category"] == "shared"}
        shared_wasm = {
            (test["package"], test["name"])
            for test in classified_node
            if test["category"] == "shared"
        }
        if shared_native != shared_wasm:
            raise ValueError(
                f"shared inventory differs: native-only={sorted(shared_native-shared_wasm)}, "
                f"wasm-only={sorted(shared_wasm-shared_native)}"
            )
        if args.require_closed and any(test["category"] == "pending" for test in classified):
            raise ValueError("pending test classifications remain")

        counts = Counter(test["category"] for test in classified)
        report = {
            "schema": 1,
            "native_count": len(native),
            "source_declaration_count": len(declarations),
            "node_count": len(node),
            "chrome_count": len(chrome),
            "category_counts": dict(sorted(counts.items())),
            "native_tests": classified,
            "node_tests": classified_node,
            "chrome_tests": classified_chrome,
            "source_declarations": declarations,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        print(
            f"Wasm inventory: native={len(native)} source={len(declarations)} "
            f"node={len(node)} chrome={len(chrome)} categories={dict(counts)}"
        )
        return 0
    except (KeyError, OSError, ValueError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(f"Wasm inventory failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
