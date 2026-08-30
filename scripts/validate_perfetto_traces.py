#!/usr/bin/env python3

from __future__ import annotations

import csv
import io
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / "target" / "perfetto-validation"
TRACE_PROCESSOR = ROOT / "scripts" / "trace_processor"


def run(command: list[str]) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )
    return result.stdout


def query(trace: pathlib.Path, sql: str) -> list[dict[str, str]]:
    environment = os.environ.copy()
    environment["HOME"] = str(ARTIFACTS / "trace-processor-home")
    result = subprocess.run(
        [str(TRACE_PROCESSOR), "--query-file", "/dev/stdin", str(trace)],
        cwd=ROOT,
        env=environment,
        input=sql,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    rows = list(csv.DictReader(io.StringIO(result.stdout)))
    if not rows:
        raise RuntimeError(f"Trace Processor returned no rows:\n{result.stdout}\n{result.stderr}")
    return rows


def main() -> int:
    if ARTIFACTS.exists():
        shutil.rmtree(ARTIFACTS)
    ARTIFACTS.mkdir(parents=True)
    multirealm = ARTIFACTS / "multirealm.pftrace"
    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "shoop_tracing",
            "--features",
            "trace-validation",
            "--example",
            "multirealm_trace",
            "--",
            str(multirealm),
        ]
    )
    result = query(
        multirealm,
        """
        SELECT
          (SELECT COUNT(*) FROM track WHERE name IN ('Window', 'Engine Worker', 'AudioWorklet')) AS realms,
          (SELECT COUNT(*) FROM slice WHERE name = 'engine.rt.callback') AS callbacks,
          (SELECT COUNT(*) FROM counter_track WHERE name LIKE '%engine.callback.load') AS counters,
          (SELECT COALESCE(SUM(value), 0) FROM stats WHERE severity = 'error' AND name LIKE '%clock%') AS clock_errors;
        """,
    )[0]
    expected = {"realms": "3", "callbacks": "2", "counters": "2", "clock_errors": "0"}
    if result != expected:
        raise RuntimeError(f"unexpected multirealm trace inventory: {result} != {expected}")

    traces = ROOT / "traces"
    if traces.exists():
        shutil.rmtree(traces)
    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "shoopdaloop",
            "--",
            "--tracing",
            "--tracing-smoke-test",
        ]
    )
    application = next(traces.glob("*.pftrace"))
    result = query(
        application,
        """
        SELECT COUNT(*) AS smoke_events FROM args
        WHERE display_value IN ('frontend.egui.tracing_started', 'frontend.egui.tracing_smoke_test');
        """,
    )[0]
    if result != {"smoke_events": "2"}:
        raise RuntimeError(f"unexpected application smoke trace: {result}")
    shutil.copy2(application, ARTIFACTS / "application.pftrace")
    shutil.rmtree(traces)
    print(f"Perfetto trace validation passed: {ARTIFACTS.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"validate_perfetto_traces.py: {error}", file=sys.stderr)
        raise SystemExit(1)
