#!/usr/bin/env python3
"""Run the shared RT fixture and regenerate its PDF from retained measurements."""
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FPGA_CLOCK_HZ = 150_000_000
BUDGET_US = 32 / 48_000 * 1_000_000
BACKENDS = ("cpu", "fpga-reference", "fpga-rtl")
PATTERNS = ("lifecycle", "mixed", "grab")


def elapsed_us(row):
    m = row["measurement"]
    cycles = m.get("fabric_cycles")
    return cycles * 1_000_000 / FPGA_CLOCK_HZ if cycles is not None else m["elapsed_ns"] / 1000


def stats(values):
    values = sorted(values)
    if not values:
        return {"count": 0}
    def percentile(p):
        return values[math.ceil(p * len(values)) - 1]
    return dict(count=len(values), min_us=values[0], mean_us=statistics.mean(values),
                median_us=statistics.median(values), stddev_us=statistics.pstdev(values),
                p95_us=percentile(.95) if len(values) >= 20 else None,
                p99_us=percentile(.99) if len(values) >= 100 else None,
                max_us=values[-1], budget_percent=max(values) / BUDGET_US * 100,
                budget_exceedances=sum(v > BUDGET_US for v in values))


def validate(run):
    if run["status"] != "completed":
        return
    f = run["fixture"]
    digest = hashlib.sha256(json.dumps(f, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    if digest != run["fixture_sha256"]:
        raise ValueError("fixture hash differs from the retained fixture")
    if (f["loops"], f["channels"], f["quantum"], f["sample_rate"], f["fpga_clock_hz"]) != (64, 2, 32, 48_000, FPGA_CLOCK_HZ):
        raise ValueError("result does not use the standard 64-stereo/Q32/150 MHz fixture")
    if f.get("version", 0) >= 3:
        if (f["fpga_chunk_frames"], f["fpga_block_chunks"]) != (32, 32):
            raise ValueError("fixture FPGA media geometry must use 32-frame chunks and 32-chunk blocks")
        if run["backend"].startswith("fpga-"):
            metadata = run["metadata"]
            if (metadata.get("chunk_frames"), metadata.get("block_chunks")) != (32, 32):
                raise ValueError("backend media geometry differs from the fixture")
            if run["backend"] == "fpga-rtl":
                profiles = {(int(p["CHUNK_FRAMES"]), int(p["BLOCK_CHUNKS"]))
                    for model in metadata.get("provenance", {}).get("models", [])
                    if "CHUNK_FRAMES" in (p := model.get("parameters", {})) and "BLOCK_CHUNKS" in p}
                if profiles != {(32, 32)}:
                    raise ValueError("elaborated RTL media geometry differs from the fixture")
        if f["pattern"] != "grab" and not run["coverage"].get("cold_block_boundary_verified"):
            raise ValueError("cold recording block-boundary growth was not verified")
    rows = run["callbacks"]
    if run["backend"] == "fpga-rtl":
        metadata = run.get("metadata", {})
        if metadata.get("fabric_hz") != FPGA_CLOCK_HZ:
            raise ValueError("RTL fabric clock differs from the report assumption")
        if metadata.get("memory_model") == "Dramsim3" and metadata.get("memory", {}).get("native", {}).get("fabric_hz") != FPGA_CLOCK_HZ:
            raise ValueError("DRAMsim3 fabric clock differs from the report assumption")
    if len(rows) != f["callbacks"] or not rows:
        raise ValueError("missing callback measurements")
    origin = rows[0]["frame"]
    for i, row in enumerate(rows):
        if (row["callback"], row["frame"]) != (i, origin + i * 32):
            raise ValueError("duplicate, missing, or reordered callback")
        if not math.isfinite(elapsed_us(row)) or elapsed_us(row) < 0:
            raise ValueError("invalid timing measurement")
        if run["backend"] == "fpga-rtl" and row["measurement"].get("fabric_cycles") is None:
            raise ValueError("RTL result lacks fabric cycle measurements")
    if len(run["events"]) != len(f["events"]) or not all(e["same_frame"] for e in run["events"]):
        raise ValueError("incomplete event verification")
    for event, expected in zip(run["events"], f["events"]):
        if (event["callback"], event["commands"], event["execution_frame"]) != (expected["callback"], expected["count"], origin + expected["callback"] * 32):
            raise ValueError("event execution differs from the fixture")
    if not run["coverage"]["complete_samples"]:
        raise ValueError("measurement capture was incomplete")


def summarize(run, filename):
    rows = run.get("callbacks", [])
    groups = {}
    for row in rows:
        group = row.get("event") or f'steady: rec={row["recording"]}, play={row["playing"]}'
        groups.setdefault(group, []).append(elapsed_us(row))
    worst = max(rows, key=elapsed_us) if rows else None
    return dict(file=filename, backend=run["backend"], pattern=run.get("fixture", {}).get("pattern"),
                repeat=run.get("repeat", 0), status=run["status"], error=run.get("error"),
                statistics=stats([elapsed_us(row) for row in rows]),
                groups={name: stats(values) for name, values in groups.items()},
                coverage=run.get("coverage"),
                worst_callback={key: worst[key] for key in ("callback", "frame", "event")} if worst else None)


def media_geometry(run):
    fixture, metadata = run.get("fixture", {}), run.get("metadata", {})
    if "fpga_chunk_frames" in fixture:
        return fixture["fpga_chunk_frames"], fixture["fpga_block_chunks"]
    if "chunk_frames" in metadata and "block_chunks" in metadata:
        return metadata["chunk_frames"], metadata["block_chunks"]
    for model in metadata.get("provenance", {}).get("models", []):
        p = model.get("parameters", {})
        if "CHUNK_FRAMES" in p and "BLOCK_CHUNKS" in p:
            return int(p["CHUNK_FRAMES"]), int(p["BLOCK_CHUNKS"])
    return None


def render(output, manifest, runs, summary):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.backends.backend_pdf import PdfPages
    import textwrap

    def page_text(pdf, title, lines):
        fig = plt.figure(figsize=(11.7, 8.3))
        fig.text(.06, .94, title, fontsize=18, va="top")
        text = "\n".join(textwrap.fill(str(line), 112) for line in lines)
        fig.text(.06, .87, text, fontsize=9, family="monospace", va="top")
        pdf.savefig(fig); plt.close(fig)

    with PdfPages(output / "report.pdf") as pdf:
        fig, ax = plt.subplots(figsize=(11.7, 8.3))
        ax.axis("off")
        ax.set_title("Standard RT benchmark: 64 stereo loops, Q=callback=32", pad=25)
        table = []
        def number(v):
            return "—" if v is None else f"{v:.2f}"
        for row in summary["runs"]:
            s = row["statistics"]
            table.append([f'{row["backend"]}\n{row["pattern"]} #{row["repeat"]}', row["status"], s["count"],
                          *[number(s.get(key)) for key in ("median_us", "p95_us", "p99_us", "max_us", "budget_percent")],
                          s.get("budget_exceedances", "—")])
        # Keep comparison pages readable for long campaigns.
        plt.close(fig)
        for start in range(0, len(table), 15):
            fig, ax = plt.subplots(figsize=(11.7, 8.3)); ax.axis("off")
            profiles = "; ".join(f'{p["chunk_frames"]}-frame chunks × {p["block_chunks"]}-chunk blocks ({p["block_frames"]} frames/block)' for p in summary["fpga_media_geometries"])
            ax.set_title("Standard RT benchmark: 64 stereo loops, Q=callback=32" + (" — incomplete campaign" if not summary["complete"] else "") + "\nFPGA media: " + (profiles or "not recorded"), pad=18)
            t = ax.table(cellText=table[start:start + 15], colLabels=["Backend / pattern / run", "Status", "N", "Median µs", "p95 µs", "p99 µs", "Max µs", "Max budget %", "Over budget"], loc="center", cellLoc="center", colWidths=[.22, .14, .07, .09, .09, .09, .09, .11, .10])
            t.auto_set_font_size(False); t.set_fontsize(8); t.scale(1, 2.2)
            fig.text(.06, .05, "FPGA RTL: cycles / 150. Rust reference: functional model CPU time.\n48 kHz reference budget: 666.67 µs. Observed maxima; no deadline enforcement. p95/p99 require N≥20/100.", fontsize=9)
            pdf.savefig(fig); plt.close(fig)

        for run in runs:
            label = f'{run["backend"]} / {run.get("fixture", {}).get("pattern")} / repeat {run.get("repeat", 0)}'
            rows = run.get("callbacks", [])
            timeline_events = [dict(callback=r["callback"], label=r["event"]) for r in rows if r.get("event")]
            if not rows:
                page_text(pdf, label, [f'Status: {run["status"]}', run.get("error") or "", run.get("metadata", {})]); continue
            fig, axes = plt.subplots(3, 1, figsize=(11.7, 8.3), sharex=True)
            fig.suptitle(label)
            x = [r["callback"] for r in rows]; y = [elapsed_us(r) for r in rows]
            axes[0].plot(x, y, linewidth=.65); axes[0].axhline(BUDGET_US, color="red", linestyle="--", label="666.67 µs reference budget")
            axes[0].set_ylabel("Processing µs"); axes[0].legend(fontsize=8)
            axes[1].plot(x, [r["control_ns"] / 1000 for r in rows], linewidth=.65); axes[1].set_ylabel("Endpoint µs")
            axes[2].plot(x, [r["measurement"]["storage_bytes"] / 1024 for r in rows], linewidth=.65); axes[2].set_ylabel("Live storage KiB"); axes[2].set_xlabel("Callback")
            for event in timeline_events:
                for ax in axes: ax.axvline(event["callback"], color="gray", alpha=.2)
                axes[0].annotate(event["label"], (event["callback"], y[event["callback"]]), rotation=90, fontsize=5)
            fig.tight_layout(); pdf.savefig(fig); plt.close(fig)

            labels = list(dict.fromkeys(e["label"] for e in timeline_events))
            if labels:
                fig, axes = plt.subplots(len(labels), 1, figsize=(11.7, max(4, len(labels) * 1.3)), squeeze=False)
                fig.suptitle(label + " — event-aligned processing")
                for ax, name in zip(axes[:, 0], labels):
                    for event in timeline_events:
                        if event["label"] == name:
                            indices = range(max(0, event["callback"] - 2), min(len(rows), event["callback"] + 9))
                            ax.plot([i - event["callback"] for i in indices], [y[i] for i in indices], marker=".", linewidth=.8)
                    ax.set_ylabel("µs"); ax.set_title(name, fontsize=9); ax.axvline(0, color="gray", linestyle="--")
                axes[-1, 0].set_xlabel("Callbacks relative to event")
                fig.tight_layout(); pdf.savefig(fig); plt.close(fig)
            s = summarize(run, "")
            lines = ["Event / steady group                 N     median µs     p95 µs     p99 µs       max µs"]
            for name, values in s["groups"].items():
                lines.append(f'{name:<30} {values["count"]:>6} {number(values.get("median_us")):>13} {number(values.get("p95_us")):>10} {number(values.get("p99_us")):>10} {number(values.get("max_us")):>12}')
            lines += ["", "Coverage: " + json.dumps(run.get("coverage", {})),
                      "Worst callback: " + json.dumps(s["worst_callback"]),
                      "Budget exceedances: " + str(s["statistics"].get("budget_exceedances")),
                      "Fixture SHA256: " + run.get("fixture_sha256", "unknown")]
            page_text(pdf, label + " — event statistics and coverage", lines)
            if run["backend"] == "fpga-rtl":
                diagnostics = ["Stage and stall intervals overlap. Time values assume 150 MHz.",
                               "Metric                                Median          Maximum"]
                for key, name in [("state_cycles", "SP active µs"), ("job_cycles", "JP active µs"),
                    ("axi_address_stall_cycles", "AXI address stall µs"), ("axi_write_stall_cycles", "AXI write stall µs"),
                    ("axi_response_stall_cycles", "AXI response stall µs"), ("dram_queue_stall_cycles", "DRAM queue stall µs"),
                    ("reads", "AXI read transfers"), ("writes", "AXI write transfers"), ("queue_peak", "Job queue peak")]:
                    values = [r["measurement"][key] for r in rows if key in r["measurement"]]
                    if values:
                        if key.endswith("_cycles"): values = [v / 150 for v in values]
                        diagnostics.append(f'{name:<35} {statistics.median(values):>12.2f} {max(values):>16.2f}')
                final = run.get("metadata", {}).get("finalization", {}).get("native", {})
                diagnostics += ["", "Final memory drain DRAM ticks: " + str(final.get("final_drain_dram_ticks", "unavailable")),
                                "Final memory idle: " + str(final.get("idle", "unavailable")),
                                "DRAM ticks are distinct from fabric cycles and are not converted with cycles/150."]
                page_text(pdf, label + " — fabric and memory diagnostics", diagnostics)
            metadata = run.get("metadata", {})
            concise = {k: v for k, v in metadata.items() if k not in ("provenance", "memory", "finalization")}
            page_text(pdf, label + " — provenance", ["Status: " + run["status"],
                "Error: " + str(run.get("error")), "Configuration: " + json.dumps(concise),
                "Source/toolchain: " + json.dumps({k: v for k, v in metadata.get("provenance", {}).items() if k != "models"}),
                "Memory profile: " + json.dumps(metadata.get("memory", {}).get("native", {}).get("configuration", {})),
                "Complete model hashes, memory statistics and final drain are retained in this run's JSON."])
        page_text(pdf, "Campaign provenance", [f"{key}: {value}" for key, value in manifest.items() if key != "files"])


def report(output):
    manifest = json.loads((output / "manifest.json").read_text())
    runs, rows = [], []
    for filename in manifest["files"]:
        run = json.loads((output / filename).read_text())
        try:
            validate(run)
        except (KeyError, ValueError, TypeError) as error:
            run["status"] = "incorrect-results"; run["error"] = str(error)
            run["callbacks"] = []; run["events"] = []
        runs.append(run); rows.append(summarize(run, filename))
    complete = manifest.get("complete", True) and len(rows) == manifest.get("expected_runs", len(rows))
    profiles = sorted({profile for run in runs if (profile := media_geometry(run)) is not None})
    summary = dict(schema_version=1, complete=complete, fpga_clock_hz=FPGA_CLOCK_HZ, budget_us=BUDGET_US,
        fpga_media_geometries=[dict(chunk_frames=c, block_chunks=b, block_frames=c*b) for c,b in profiles], runs=rows)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    with (output / "callbacks.csv").open("w", newline="") as stream:
        diagnostics = ["reads", "writes", "reclaimed", "state_cycles", "job_cycles", "queue_peak", "axi_address_stall_cycles", "axi_write_stall_cycles", "axi_response_stall_cycles", "dram_queue_stall_cycles"]
        fields = ["backend", "pattern", "repeat", "callback", "frame", "event", "elapsed_us", "control_ns", "fabric_cycles", "storage_bytes", "recording", "playing", "wraps", "planned_slots_retired", "takes_replaced", *diagnostics]
        writer = csv.DictWriter(stream, fieldnames=fields); writer.writeheader()
        for run in runs:
            for row in run.get("callbacks", []):
                writer.writerow(dict(backend=run["backend"], pattern=run["fixture"]["pattern"], repeat=run.get("repeat", 0),
                    **{k: row[k] for k in ("callback", "frame", "event", "control_ns", "recording", "playing", "wraps")},
                    elapsed_us=elapsed_us(row), fabric_cycles=row["measurement"].get("fabric_cycles"), storage_bytes=row["measurement"]["storage_bytes"],
                    planned_slots_retired=row.get("planned_slots_retired"), takes_replaced=row.get("takes_replaced"),
                    **{key: row["measurement"].get(key) for key in diagnostics}))
    render(output, manifest, runs, summary)
    return complete and all(row["status"] == "completed" or (row["status"] == "unsupported" and row["pattern"] == "grab") for row in rows)


def capture(*args):
    try:
        return subprocess.check_output(args, cwd=ROOT, stderr=subprocess.DEVNULL, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backends", default=",".join(BACKENDS))
    parser.add_argument("--patterns", default=",".join(PATTERNS))
    parser.add_argument("--preset", choices=("smoke", "standard"), default="standard")
    parser.add_argument("--repeats", type=int)
    parser.add_argument("--output", type=Path, default=ROOT / "work/rt-benchmark")
    parser.add_argument("--report-only", action="store_true")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--memory-model", choices=("dramsim3", "fixed"), default="dramsim3")
    args = parser.parse_args()
    if importlib.util.find_spec("matplotlib") is None:
        parser.error("Matplotlib is required for the report; install 'matplotlib>=3.9,<4' before starting a campaign")
    output = args.output.resolve(); output.mkdir(parents=True, exist_ok=True)
    if args.report_only:
        return 0 if report(output) else 1
    backends = args.backends.split(","); patterns = args.patterns.split(",")
    repeats = {b: args.repeats if args.repeats is not None else (1 if args.preset == "smoke" or b == "fpga-rtl" else 3) for b in backends}
    if any(n < 1 for n in repeats.values()) or not backends or len(set(backends)) != len(backends) or len(set(patterns)) != len(patterns) or any(b not in BACKENDS for b in backends) or any(p not in PATTERNS for p in patterns):
        parser.error("select known backends/patterns and positive repeats")
    if any(output.iterdir()):
        parser.error("output already contains a campaign; select a fresh directory or --report-only")
    manifest = dict(schema_version=1, created_at=datetime.now(timezone.utc).isoformat(), revision=capture("git", "rev-parse", "HEAD"),
        worktree=capture("git", "status", "--porcelain"), rustc=capture("rustc", "--version"),
        python=capture("python3", "--version"), platform=capture("uname", "-a"),
        affinity=sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        preset=args.preset, repeats=repeats, backends=backends, patterns=patterns,
        expected_runs=sum(repeats.values()) * len(patterns), complete=False,
        fpga_clock_hz=FPGA_CLOCK_HZ, memory_model=args.memory_model, files=[])
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    missing_rtl = None
    version = capture(os.environ.get("SHOOP_VERILATOR", "verilator"), "--version") if "fpga-rtl" in backends else None
    if "fpga-rtl" in backends and (not version or version.split()[:2] != ["Verilator", "5.032"]):
        missing_rtl = "Verilator 5.032 is required; run scripts/build_verilator.sh and select its executable"
    if "fpga-rtl" in backends and args.memory_model == "dramsim3" and not (ROOT / "work/dramsim3/build/libshoop_dram.so").is_file():
        missing_rtl = "DRAMsim3 is required; run python3 fpga/axi_memory/build_dramsim3.py"
    binary = args.binary.resolve() if args.binary else ROOT / "target/release" / ("shoop_rt_benchmark.exe" if os.name == "nt" else "shoop_rt_benchmark")
    build_error = None
    if not args.binary:
        command = ["cargo", "build", "--locked", "--release", "-p", "shoop_rt_benchmark"]
        if "fpga-rtl" in backends and not missing_rtl: command += ["--features", "rtl"]
        with (output / "build.log").open("w") as log:
            try:
                subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
            except (OSError, subprocess.CalledProcessError) as error:
                build_error = f"{error}; see build.log"
    manifest["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest() if binary.is_file() else None
    for repeat in range(max(repeats.values())):
        for backend in backends[::1 if repeat % 2 == 0 else -1]:
            if repeat >= repeats[backend]:
                continue
            for pattern in patterns:
                filename = f"{backend}-{pattern}-{repeat}.json"; path = output / filename
                print(f"{backend} / {pattern} / repeat {repeat}", flush=True)
                error = build_error or (missing_rtl if backend == "fpga-rtl" else None)
                if binary.is_file() and hashlib.sha256(binary.read_bytes()).hexdigest() != manifest["binary_sha256"]:
                    error = "benchmark binary changed during this campaign; use a fresh output directory"
                if error:
                    value = dict(backend=backend, fixture=dict(pattern=pattern), status="missing-prerequisite" if not build_error else "build-failure", error=error)
                else:
                    command = [str(binary), "--backend", backend, "--preset", args.preset, "--pattern", pattern, "--output", str(path)]
                    if backend == "fpga-rtl": command += ["--memory-model", args.memory_model]
                    with path.with_suffix(".log").open("w") as log:
                        try:
                            result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
                            value = json.loads(path.read_text()) if path.exists() else dict(backend=backend, fixture=dict(pattern=pattern), status="processing-failure", error=f"exit {result.returncode}; see {path.with_suffix('.log').name}")
                            if result.returncode and value["status"] in ("completed", "unsupported"):
                                value["status"] = "processing-failure"; value["error"] = f"unexpected exit {result.returncode}; see {path.with_suffix('.log').name}"
                        except json.JSONDecodeError as exc:
                            if path.exists(): path.rename(path.with_suffix(".partial.json"))
                            value = dict(backend=backend, fixture=dict(pattern=pattern), status="processing-failure", error=f"invalid or truncated backend output: {exc}")
                        except OSError as exc:
                            value = dict(backend=backend, fixture=dict(pattern=pattern), status="missing-prerequisite", error=str(exc))
                value["repeat"] = repeat; path.write_text(json.dumps(value, indent=2) + "\n")
                f = value.get("fixture", {})
                if value["status"] in ("completed", "unsupported") and (f.get("version"), f.get("fpga_chunk_frames"), f.get("fpga_block_chunks")) != (3, 32, 32):
                    path.rename(path.with_suffix(".incompatible.json"))
                    value = dict(backend=backend, fixture=dict(pattern=pattern), repeat=repeat, status="incorrect-results",
                        error="new campaigns require fixture revision 3 with 32-frame chunks and 32-chunk blocks; original result retained as .incompatible.json")
                    path.write_text(json.dumps(value, indent=2) + "\n")
                    f = value["fixture"]
                if "fpga_chunk_frames" in f:
                    manifest["fpga_media_geometry"] = dict(chunk_frames=f["fpga_chunk_frames"], block_chunks=f["fpga_block_chunks"])
                manifest["files"].append(filename)
                (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    manifest["complete"] = True
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return 0 if report(output) else 1


if __name__ == "__main__":
    raise SystemExit(main())
