#!/usr/bin/env python3

from __future__ import annotations

import dataclasses
import re
import xml.etree.ElementTree as ET
from pathlib import Path

ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
TEST_LINE = re.compile(r"^test (?P<name>.+?) \.\.\. (?P<status>ok|FAIL|ignored(?:, .+)?)$")
SUMMARY = re.compile(
    r"^test result: (?P<result>ok|FAILED)\. "
    r"(?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored; (?P<filtered>\d+) filtered out;"
)


@dataclasses.dataclass(frozen=True)
class Case:
    name: str
    status: str
    detail: str = ""


@dataclasses.dataclass(frozen=True)
class ParsedReport:
    cases: tuple[Case, ...]
    listed: int
    summaries: int
    malformed: tuple[str, ...]

    @property
    def failed(self) -> int:
        return sum(case.status == "failed" for case in self.cases)

    @property
    def ignored(self) -> int:
        return sum(case.status == "ignored" for case in self.cases)



def parse_output(output: str) -> ParsedReport:
    clean = ANSI.sub("", output).replace("\r\n", "\n")
    cases: list[Case] = []
    totals = [0, 0, 0]
    summaries = 0
    malformed: list[str] = []
    for line in clean.splitlines():
        match = TEST_LINE.match(line)
        if match:
            status_text = match.group("status")
            status = (
                "passed"
                if status_text == "ok"
                else "failed"
                if status_text == "FAIL"
                else "ignored"
            )
            detail = status_text.split(", ", 1)[1] if ", " in status_text else ""
            cases.append(Case(match.group("name"), status, detail))
            continue
        match = SUMMARY.match(line)
        if match:
            summaries += 1
            totals[0] += int(match.group("passed"))
            totals[1] += int(match.group("failed"))
            totals[2] += int(match.group("ignored"))

    observed = [
        sum(case.status == "passed" for case in cases),
        sum(case.status == "failed" for case in cases),
        sum(case.status == "ignored" for case in cases),
    ]
    if summaries == 0:
        malformed.append("runner emitted no test-result summary")
    if observed != totals:
        malformed.append(f"test-result counts {totals} do not match parsed cases {observed}")
    names = [case.name for case in cases]
    if len(names) != len(set(names)):
        malformed.append("runner emitted duplicate testcase names within one package")
    return ParsedReport(tuple(cases), sum(totals), summaries, tuple(malformed))



def write_junit(
    destination: Path,
    *,
    package: str,
    runtime: str,
    profile: str,
    command: list[str],
    returncode: int,
    elapsed_seconds: float,
    output: str,
) -> ParsedReport:
    parsed = parse_output(output)
    synthetic: list[str] = list(parsed.malformed)
    if parsed.listed == 0:
        synthetic.append("runner discovered zero tests")
    if returncode != 0 and parsed.failed == 0:
        synthetic.append(f"runner exited with status {returncode} without a testcase failure")
    if returncode == 0 and parsed.failed:
        synthetic.append("runner returned success while reporting failed testcases")

    failures = parsed.failed + len(synthetic)
    suite = ET.Element(
        "testsuite",
        {
            "name": f"{package}.{runtime}",
            "tests": str(len(parsed.cases) + len(synthetic)),
            "failures": str(failures),
            "skipped": str(parsed.ignored),
            "time": f"{elapsed_seconds:.3f}",
        },
    )
    properties = ET.SubElement(suite, "properties")
    for name, value in (
        ("package", package),
        ("runtime", runtime),
        ("profile", profile),
        ("command", " ".join(command)),
        ("returncode", str(returncode)),
    ):
        ET.SubElement(properties, "property", {"name": name, "value": value})

    for case in parsed.cases:
        node = ET.SubElement(
            suite,
            "testcase",
            {"name": case.name, "classname": package, "time": "0"},
        )
        if case.status == "failed":
            failure = ET.SubElement(node, "failure", {"message": "Wasm testcase failed"})
            failure.text = output
        elif case.status == "ignored":
            ET.SubElement(node, "skipped", {"message": case.detail or "ignored"})

    for index, message in enumerate(synthetic, 1):
        node = ET.SubElement(
            suite,
            "testcase",
            {"name": f"runner_failure_{index}", "classname": f"{package}.runner", "time": "0"},
        )
        failure = ET.SubElement(node, "failure", {"message": message})
        failure.text = output

    out = ET.SubElement(suite, "system-out")
    out.text = output
    destination.parent.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(suite).write(destination, encoding="utf-8", xml_declaration=True)
    return parsed
