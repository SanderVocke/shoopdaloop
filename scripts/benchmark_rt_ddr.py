#!/usr/bin/env python3
"""Compare ideal AXI memory with DRAMsim3 using one short common RTL fixture."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import csv
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time

import benchmark_rt as benchmark

MODELS = ("ideal", "dramsim3")
LABELS = {"ideal": "Ideal (zero added delay)", "dramsim3": "DRAMsim3"}


def report(output):
    manifest = json.loads((output / "manifest.json").read_text())
    cases, runs, errors = {}, {}, []
    for model in MODELS:
        case = dict(memory_model=model, status="incorrect-results")
        try:
            record = manifest["runs"][model]
            if record["exit_code"]:
                raise ValueError(f'capture exit {record["exit_code"]}; inspect {model}.log')
            child = json.loads((output / model / "manifest.json").read_text())
            if not child["complete"] or child["expected_runs"] != 1 or len(child["files"]) != 1:
                raise ValueError("comparison requires one complete RTL case per memory model")
            run = json.loads((output / model / child["files"][0]).read_text())
            benchmark.validate(run)
            fixture = run["fixture"]
            if run["backend"] != "fpga-rtl" or run["status"] != "completed" or (fixture["media"], fixture["pattern"], fixture["callbacks"]) != (manifest["media"], "ddr", 38):
                raise ValueError("comparison requires the 38-callback RTL-only DDR fixture")
            if child["binary_sha256"] != manifest["binary_sha256"]:
                raise ValueError("comparison cases must use the same frozen executable")
            if run["metadata"]["memory_model"] != {"ideal": "Ideal", "dramsim3": "Dramsim3"}[model]:
                raise ValueError("captured memory model differs from the requested comparison")
            runs[model] = run
            case.update(status="completed", file=f'{model}/{child["files"][0]}', fixture_sha256=run["fixture_sha256"],
                statistics=benchmark.stats([benchmark.elapsed_us(row) for row in run["callbacks"]]),
                total_cycles=sum(row["measurement"]["fabric_cycles"] for row in run["callbacks"]),
                wall_s=record["wall_s"], dram_config=Path(child["dram_config"]).name if child.get("dram_config") else None,
                dram_config_sha256=child.get("dram_config_sha256"))
        except (OSError, ValueError, KeyError, TypeError) as error:
            case["error"] = str(error)
            errors.append(f'{model}: {error}')
        cases[model] = case
    same_fixture = len(runs) == 2 and len({run["fixture_sha256"] for run in runs.values()}) == 1
    rows = []
    if len(runs) == 2:
        if not same_fixture:
            errors.append("fixture hashes differ across the two memory models")
        for ideal, dram in zip(runs["ideal"]["callbacks"], runs["dramsim3"]["callbacks"]):
            fields = ("callback", "frame", "event", "recording", "playing", "midi_input_events", "midi_output_events", "output_peak")
            if any(ideal.get(key) != dram.get(key) for key in fields):
                errors.append(f'functional workload differs at callback {ideal["callback"]}')
            a, b = benchmark.elapsed_us(ideal), benchmark.elapsed_us(dram)
            rows.append(dict(callback=ideal["callback"], event=ideal["event"],
                ideal_cycles=ideal["measurement"]["fabric_cycles"], dramsim3_cycles=dram["measurement"]["fabric_cycles"],
                ideal_us=a, dramsim3_us=b, delta_us=b-a, ratio=b/a if a else None))
    passed = manifest["complete"] and same_fixture and not errors
    summary = dict(schema_version=1, kind="rtl-ddr-comparison", media=manifest["media"], passed=passed,
        fixture_comparisons_match=same_fixture, fpga_clock_hz=benchmark.FPGA_CLOCK_HZ,
        cases=cases, callbacks=rows, errors=errors)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    with (output / "callbacks.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=("callback", "event", "ideal_cycles", "dramsim3_cycles", "ideal_us", "dramsim3_us", "delta_us", "ratio"))
        writer.writeheader(); writer.writerows(rows)
    render(output, manifest, summary)
    return passed


def render(output, manifest, summary):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import textwrap

    fig = plt.figure(figsize=(11.7, 8.3))
    fig.text(.05, .94, "RTL DDR comparison — " + manifest["media"] + " only", fontsize=19, weight="bold")
    fig.text(.05, .89, "64 loops · stereo when audio is enabled · Q=callback=32 · 32-frame chunks × 32-chunk blocks\n"
        "38 deterministic callbacks per model · complete-callback fabric cycles / 150 = µs", fontsize=10)
    fig.text(.05, .84, "VALIDATED: SAME FIXTURE, EXECUTABLE AND OUTPUT WORK" if summary["passed"] else "FAILED / INCOMPLETE COMPARISON",
        fontsize=11, weight="bold", color="#176b3a" if summary["passed"] else "#b22222")

    def table(rect, headers, values, widths):
        axis = fig.add_axes(rect); axis.axis("off")
        grid = axis.table(cellText=values, colLabels=headers, colWidths=widths, bbox=[0, 0, 1, 1])
        grid.auto_set_font_size(False); grid.set_fontsize(9)
        for (row, col), cell in grid.get_celld().items():
            cell.set_edgecolor("#ccd7e0"); cell.set_linewidth(.5)
            if row == 0:
                cell.set_facecolor("#1b3c55"); cell.set_text_props(color="white", weight="bold")
            else:
                cell.set_facecolor("#eef3f7" if row % 2 else "white")

    totals = []
    for model, case in summary["cases"].items():
        if case["status"] == "completed":
            totals.append([LABELS[model], f'{case["total_cycles"]:,}', f'{case["statistics"]["max_us"]:,.2f}', f'{case["wall_s"]:.2f}'])
        else:
            totals.append([LABELS[model], "failed", "—", "—"])
    table([.05, .70, .90, .10], ["Memory service", "Total fabric cycles", "Worst callback µs", "Host wall seconds"], totals, [.37, .23, .22, .18])
    rows = summary["callbacks"]
    if rows:
        indices = [("Cold record all", 0), ("Audio block growth" if manifest["media"] == "audio" else "MIDI event-block growth", 32 if manifest["media"] == "audio" else 21),
            ("Play all / dense merge", 33), ("Retake all", 35)]
        peak = max(rows, key=lambda row: row["dramsim3_cycles"])["callback"]
        if peak not in {index for _, index in indices}: indices.append(("Worst DRAM callback", peak))
        selected = [[label, index, f'{rows[index]["ideal_cycles"]:,}', f'{rows[index]["dramsim3_cycles"]:,}',
            f'{rows[index]["ideal_us"]:,.2f}', f'{rows[index]["dramsim3_us"]:,.2f}', f'{rows[index]["delta_us"]:+,.2f}',
            f'{rows[index]["ratio"]:.3f}×' if rows[index]["ratio"] is not None else "—"] for label, index in indices]
        table([.05, .39, .90, .24], ["Scenario", "Callback", "Ideal cycles", "DRAM cycles", "Ideal µs", "DRAM µs", "Δ µs", "DRAM/ideal"], selected,
            [.24, .08, .13, .13, .11, .11, .10, .10])
    dram = summary["cases"].get("dramsim3", {})
    config = dram.get("dram_config") or "unavailable"
    digest = (dram.get("dram_config_sha256") or "unavailable")[:16]
    notes = ["Ideal memory adds zero service latency, refresh or bank contention; AXI handshakes and transfers still consume cycles. Fixed-delay (7 cycles) is a separate diagnostic mode.",
        f"Simulated DDR: {config} (SHA256 prefix {digest}). Both AXI ports share one controller in either model. µs above are simulated hardware time, separate from overlapping child host runtimes.",
        "Every output sample/message is checked. Cold block growth, the dense 64-loop MIDI merge (MIDI only), simultaneous retake and final stop are required. Quiet playback, wraps, planned transitions and mixed recording/playback are intentionally omitted."]
    notes += summary["errors"][:2]
    y = .33
    for note in notes:
        lines = textwrap.wrap(note, 145)
        fig.text(.05, y, "\n".join(lines), fontsize=8.5, va="top")
        y -= .018 * len(lines) + .015
    fig.text(.05, .055, f'Source: {manifest["revision"]}\nBinary SHA256: {manifest.get("binary_sha256") or "unavailable"}\nAll callbacks: callbacks.csv; validation/errors: summary.json; raw runs and provenance: ideal/ and dramsim3/.', fontsize=8)
    fig.savefig(output / "report.pdf"); plt.close(fig)


def main(argv=None):
    started = time.perf_counter()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--media", choices=("audio", "midi"), default="audio")
    parser.add_argument("--output", type=Path, default=benchmark.ROOT / "work/rt-ddr-comparison")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--jobs", type=int, choices=(1, 2), default=2)
    parser.add_argument("--dram-config", type=Path)
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args(argv)
    if importlib.util.find_spec("matplotlib") is None:
        parser.error("Matplotlib is required for the report; install 'matplotlib>=3.9,<4'")
    output = args.output.resolve(); output.mkdir(parents=True, exist_ok=True)
    if args.report_only: return 0 if report(output) else 1
    if any(output.iterdir()): parser.error("select a fresh output directory or --report-only")
    manifest = dict(schema_version=1, kind="rtl-ddr-comparison", complete=False, media=args.media,
        created_at=datetime.now(timezone.utc).isoformat(), revision=benchmark.capture("git", "rev-parse", "HEAD"),
        worktree=benchmark.capture("git", "status", "--porcelain"), runs={})
    phases = {}
    binary = args.binary.resolve() if args.binary else benchmark.ROOT / "target/release/shoop_rt_benchmark"
    build_started = time.perf_counter()
    build_exit = 0
    if not args.binary:
        with (output / "build.log").open("w") as log:
            try:
                build_exit = subprocess.run(["cargo", "build", "--locked", "--release", "-p", "shoop_rt_benchmark", "--features", "rtl"], cwd=benchmark.ROOT, stdout=log, stderr=subprocess.STDOUT).returncode
            except OSError as error:
                log.write(str(error)); build_exit = -1
    phases["build"] = time.perf_counter() - build_started
    manifest["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest() if binary.is_file() else None

    def run(model):
        child_started = time.perf_counter()
        command = [sys.executable, str(benchmark.ROOT / "scripts/benchmark_rt.py"), "--backends", "fpga-rtl", "--media", args.media,
            "--preset", "tiny", "--patterns", "ddr", "--repeats", "1", "--rtl-jobs", "1", "--host-profile",
            "--memory-model", model, "--binary", str(binary), "--output", str(output / model)]
        if args.dram_config: command += ["--dram-config", str(args.dram_config.resolve())]
        print(f'{args.media}: {model} RTL comparison', flush=True)
        with (output / f"{model}.log").open("w") as log:
            try:
                code = build_exit or subprocess.run(command, cwd=benchmark.ROOT, stdout=log, stderr=subprocess.STDOUT).returncode
            except OSError as error:
                log.write(str(error)); code = -1
        return model, dict(exit_code=code, wall_s=time.perf_counter()-child_started)

    cases_started = time.perf_counter()
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        manifest["runs"] = dict(executor.map(run, MODELS))
    phases["paired_rtl_cases"] = time.perf_counter() - cases_started
    manifest["complete"] = True
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    report_started = time.perf_counter()
    passed = report(output)
    phases["report"] = time.perf_counter() - report_started
    elapsed = time.perf_counter() - started
    phases["setup_and_orchestration"] = max(0., elapsed - sum(phases.values()))
    (output / "runtime.json").write_text(json.dumps(dict(complete=True, passed=passed, elapsed_s=elapsed, phases_s=phases), indent=2) + "\n")
    print(f'Comparison {"passed" if passed else "FAILED"}: {elapsed:.2f} host seconds; {output / "report.pdf"}')
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
