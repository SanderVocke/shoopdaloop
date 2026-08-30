#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime
import json
import os
import pathlib
import platform
import statistics
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / "examples" / "tracing_audio_benchmark"
MODES = ("disabled", "coarse", "detailed")


def run(command: list[str]) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=os.environ.copy(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )
    return result.stdout.strip()


def parse_result(output: str) -> dict[str, int | float | str]:
    line = next((line for line in output.splitlines() if line.startswith("RESULT ")), None)
    if line is None:
        raise RuntimeError(f"benchmark emitted no RESULT line:\n{output}")
    result: dict[str, int | float | str] = {}
    for item in line.removeprefix("RESULT ").split():
        name, value = item.split("=", 1)
        if name in {"mode"}:
            result[name] = value
        elif "." in value:
            result[name] = float(value)
        else:
            result[name] = int(value)
    return result


def machine_description() -> dict[str, str]:
    cpu = platform.processor()
    if pathlib.Path("/proc/cpuinfo").is_file():
        for line in pathlib.Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    return {
        "platform": platform.platform(),
        "processor": cpu or "unknown",
        "python": platform.python_version(),
        "rustc": run(["rustc", "--version"]),
    }


def median(rows: list[dict], name: str) -> float:
    return statistics.median(float(row[name]) for row in rows)


def markdown(report: dict) -> str:
    lines = [
        "# Tracy audio tracing baseline",
        "",
        "This is a same-process release-mode diagnostic benchmark, not a hardware-independent",
        "performance claim. It exercises a fixed 16-loop dummy-engine graph at 48 kHz with",
        "128-frame quanta. Callback timing is measured around every cycle in every mode; the",
        "engine's own timing is active only when tracing is requested.",
        "",
        f"- UTC: `{report['created_utc']}`",
        f"- Git: `{report['git_head']}`",
        f"- Platform: `{report['machine']['platform']}`",
        f"- CPU: `{report['machine']['processor']}`",
        f"- Rust: `{report['machine']['rustc']}`",
        f"- Tracy client: `{report['components']['tracy_client']}`",
        f"- Embedded capture/query: `{report['components']['tracy_extensions']}`",
        f"- Inspected Perfetto candidate: `{report['components']['perfetto_everywhere']}`",
        f"- Repetitions: {report['repetitions']}",
        f"- Measured cycles per repetition: {report['cycles']}",
        "",
        "| Mode | Median cycles/s | Median p50 µs | Median p95 µs | Median p99 µs | Median max µs | Median budget overruns |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    for mode in MODES:
        rows = [row for row in report["runs"] if row["mode"] == mode]
        lines.append(
            f"| {mode} | {median(rows, 'cycles_per_second'):.0f} | "
            f"{median(rows, 'callback_p50_ns') / 1000:.2f} | "
            f"{median(rows, 'callback_p95_ns') / 1000:.2f} | "
            f"{median(rows, 'callback_p99_ns') / 1000:.2f} | "
            f"{median(rows, 'callback_max_ns') / 1000:.2f} | "
            f"{median(rows, 'external_budget_overruns'):.0f} |"
        )
    lines.extend(
        [
            "",
            "Raw repetitions are in the adjacent JSON file. Scheduler noise and machine load",
            "remain visible in maxima; compare medians and upper percentiles together, and rerun",
            "the identical command for Perfetto on the same machine.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cycles", type=int, default=20_000)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument(
        "--output-prefix",
        type=pathlib.Path,
        default=ROOT / "artifacts" / "tracy-audio-baseline",
    )
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    if args.cycles <= 0 or args.repetitions <= 0:
        parser.error("cycles and repetitions must be positive")

    if not args.skip_build:
        print(run(["cargo", "build", "--release", "-p", "shoop_engine", "--example", "tracing_audio_benchmark"]))
    if not BINARY.is_file():
        raise RuntimeError(f"benchmark binary is missing: {BINARY}")

    runs = []
    for repetition in range(1, args.repetitions + 1):
        for mode in MODES:
            print(f"benchmark repetition={repetition} mode={mode}", flush=True)
            result = parse_result(run([str(BINARY), mode, str(args.cycles)]))
            result["repetition"] = repetition
            runs.append(result)

    report = {
        "schema": 1,
        "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "git_head": run(["git", "rev-parse", "HEAD"]),
        "machine": machine_description(),
        "cycles": args.cycles,
        "repetitions": args.repetitions,
        "components": {
            "tracy_client": "0.18.4 / Tracy 0.13.1",
            "tracy_extensions": "v0.7.0",
            "perfetto_everywhere": "48ed779",
        },
        "runs": runs,
    }
    prefix = args.output_prefix
    if not prefix.is_absolute():
        prefix = ROOT / prefix
    prefix.parent.mkdir(parents=True, exist_ok=True)
    prefix.with_suffix(".json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    prefix.with_suffix(".md").write_text(markdown(report))
    print(prefix.with_suffix(".md").read_text())
    print(f"raw: {prefix.with_suffix('.json')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
