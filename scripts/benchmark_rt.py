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
    from collections import Counter, defaultdict

    def number(value):
        return "—" if value is None else f"{value:,.2f}"

    def acceptable(run):
        return run["status"] == "completed" or (run["status"] == "unsupported" and run.get("fixture", {}).get("pattern") == "grab")

    def status(group):
        counts = Counter(r["status"] for r in group)
        return ", ".join(f"{v} {k}" for k, v in sorted(counts.items()))

    grouped = defaultdict(list)
    for run in runs:
        grouped[(run["backend"], run.get("fixture", {}).get("pattern", "unknown"))].append(run)
    rtl = [r for r in runs if r["backend"] == "fpga-rtl" and r["status"] == "completed" and r.get("callbacks")]
    hosts = [r for r in runs if r["backend"] != "fpga-rtl" and r["status"] == "completed" and r.get("callbacks")]
    completed = [r for r in runs if r["status"] == "completed"]
    all_valid = summary["complete"] and all(acceptable(r) for r in runs)
    counts = Counter(r["status"] for r in runs)
    profiles = summary["fpga_media_geometries"]
    if len(profiles) == 1:
        p = profiles[0]
        geometry = f'FPGA media: {p["chunk_frames"]}-frame chunks × {p["block_chunks"]}-chunk blocks ({p["block_frames"]:,} frames/block)'
    else:
        geometry = f"FPGA media: {len(profiles)} recorded profiles; see run JSON" if profiles else "FPGA geometry not recorded"

    def page(title, subtitle=""):
        fig = plt.figure(figsize=(11.7, 8.3))
        fig.text(.055, .945, title, fontsize=18, weight="bold", va="top")
        if subtitle:
            fig.text(.055, .892, subtitle, fontsize=9.5, va="top")
        fig.text(.055, .035, "Complete callback time · RTL cycles / 150 · 666.67 µs reference budget · observed maxima", fontsize=8, color="#555555")
        fig.text(.945, .035, str(pdf.get_pagecount() + 1), fontsize=8, ha="right", color="#555555")
        return fig

    def table(fig, bounds, headers, rows, widths=None, size=9):
        ax = fig.add_axes(bounds); ax.axis("off")
        if not rows:
            ax.text(0, .8, "No completed measurements available.", fontsize=11)
            return
        if widths:
            widths = [width / sum(widths) for width in widths]
        t = ax.table(cellText=rows, colLabels=headers, cellLoc="left", loc="upper left", bbox=[0, 0, 1, 1], colWidths=widths)
        t.auto_set_font_size(False); t.set_fontsize(size)
        for (row, col), cell in t.get_celld().items():
            cell.set_edgecolor("#D0D7DE"); cell.set_linewidth(.5)
            if row == 0:
                cell.set_facecolor("#193B56"); cell.set_text_props(color="white", weight="bold")
            else:
                cell.set_facecolor("#F0F5F8" if row % 2 else "white")
        return t

    def block(fig, x, y, text, width=105, lines=12, size=9):
        wrapped = []
        for line in text:
            wrapped.extend(textwrap.wrap(str(line), width=width, break_long_words=True) or [""])
        if len(wrapped) > lines:
            wrapped = wrapped[:lines - 1] + ["… Full details retained in JSON."]
        fig.text(x, y, "\n".join(wrapped), fontsize=size, va="top", linespacing=1.4)

    def samples(group):
        return [row for run in group if run["status"] == "completed" for row in run.get("callbacks", [])]

    def group_name(row):
        if row.get("event"):
            return row["event"]
        return f'steady: {row.get("recording", 0)} record / {row.get("playing", 0)} play'

    def pooled_note(group):
        hashes = {r.get("fixture_sha256") for r in group if r["status"] == "completed"}
        return " *" if len(hashes) > 1 else ""

    with PdfPages(output / "report.pdf") as pdf:
        fig = page("FPGA RTL — performance overview", "64 stereo audio loops · Q=callback=32 · 48 kHz\n" + geometry)
        campaign = f'{len(runs)}/{manifest.get("expected_runs", len(runs))} run slots · {counts["completed"]} completed · {counts["unsupported"]} unsupported · {sum(v for k,v in counts.items() if k not in ("completed", "unsupported"))} failed'
        fig.text(.055, .817, ("VALIDATED CAMPAIGN" if all_valid else "INCOMPLETE / INVALID CAMPAIGN") + " — " + campaign,
                 fontsize=10, weight="bold", color="#176B43" if all_valid else "#AE2C2C")
        overview = []
        for (backend, pattern), group in sorted(grouped.items()):
            if backend != "fpga-rtl": continue
            rows = samples(group); s = stats([elapsed_us(row) for row in rows])
            maximum = max((r["measurement"]["fabric_cycles"] for r in rows), default=None)
            overview.append([pattern + pooled_note(group), status(group), f'{s["count"]:,}',
                *[number(s.get(k)) for k in ("median_us", "p95_us", "p99_us", "max_us")],
                "—" if maximum is None else f"{maximum:,}", s.get("budget_exceedances", "—")])
        table(fig, [.055, .605, .89, .175], ["Pattern", "Run status", "N", "Median µs", "p95 µs", "p99 µs", "Max µs", "Max cycles", "Over budget"], overview,
              [.11, .19, .08, .10, .09, .09, .09, .14, .11], size=8.5)
        ax = fig.add_axes([.09, .185, .82, .34])
        if rtl:
            for pattern in sorted({r["fixture"]["pattern"] for r in rtl}):
                values = sorted(elapsed_us(row) for r in rtl if r["fixture"]["pattern"] == pattern for row in r["callbacks"])
                ax.step(values, [(i + 1) * 100 / len(values) for i in range(len(values))], where="post", label=f"{pattern} (N={len(values):,})")
            ax.axvline(BUDGET_US, color="#B9392D", linestyle="--", linewidth=1, label="Reference budget")
            ax.set_xlabel("Complete callback time (µs)"); ax.set_ylabel("Callbacks at or below (%)")
            ax.set_ylim(0, 101); ax.set_xlim(left=0); ax.grid(alpha=.2); ax.legend(fontsize=8, loc="lower right")
            ax.set_title("Processing-time distribution", fontsize=11)
        else:
            ax.axis("off"); ax.text(0, .6, "RTL was not selected or has no completed results.\nSee validation/status details for missing or failed runs.", fontsize=12)
        block(fig, .055, .125, ["Percentiles require N≥20/100 for p95/p99. Distributions pool completed repeats; detailed per-repeat statistics remain in summary.json.",
            "* Multiple fixture hashes in a group: values span different fixtures; consult raw provenance before comparing." if any(pooled_note(g) for g in grouped.values()) else "No deadlines enforced. Unsupported FPGA grabs are not replaced with a cheaper operation."], lines=3, size=8.5)
        pdf.savefig(fig); plt.close(fig)

        if rtl:
            event_groups = defaultdict(list)
            for run in rtl:
                for row in run["callbacks"]:
                    event_groups[(run["fixture"]["pattern"], group_name(row))].append(elapsed_us(row))
            selected = sorted(event_groups.items(), key=lambda item: max(item[1]), reverse=True)[:22]
            selected.sort(key=lambda item: (item[0][0], -max(item[1])))
            fig = page("FPGA RTL — stress events and steady processing", f"{len(selected)} highest-maximum event/steady populations of {len(event_groups)} · counts include completed repeats")
            event_rows = []
            for (pattern, name), values in selected:
                s = stats(values)
                event_rows.append([pattern, name, f'{s["count"]:,}', *[number(s.get(k)) for k in ("median_us", "p95_us", "p99_us", "max_us")], s["budget_exceedances"]])
            table(fig, [.055, .15, .89, .69], ["Pattern", "Event / steady population", "N", "Median µs", "p95 µs", "p99 µs", "Max µs", "Over budget"], event_rows,
                  [.12, .31, .07, .10, .10, .10, .10, .10], size=8.5)
            block(fig, .055, .105, ["Cold starts and re-recording are separate populations. Observed storage/plan events remain distinct from steady work.",
                "All populations and samples remain in summary.json and callbacks.csv. Small event populations have no extreme percentiles."], width=135, lines=4, size=8.5)
            pdf.savefig(fig); plt.close(fig)

            worst = {}
            for run in rtl:
                pattern = run["fixture"]["pattern"]; row = max(run["callbacks"], key=elapsed_us)
                if pattern not in worst or elapsed_us(row) > elapsed_us(worst[pattern][1]): worst[pattern] = (run, row)
            fig = page("FPGA RTL — worst callbacks and processing cost", "One worst observed callback per pattern; each first-callback comparison uses the same prepared run")
            peak_rows = []
            for pattern, (run, row) in sorted(worst.items()):
                peak_rows.append([pattern, run.get("repeat", 0), row["callback"], f'{row["frame"]:,}', row.get("event") or "steady", f'{row["measurement"]["fabric_cycles"]:,}', number(elapsed_us(row))])
            table(fig, [.055, .69, .89, .14], ["Pattern", "Repeat", "Callback", "Frame", "Trigger", "Cycles", "µs"], peak_rows,
                  [.12, .07, .10, .11, .32, .15, .13], size=9)
            headers = ["Metric"]
            for pattern in sorted(worst): headers += [pattern + "\nfirst", pattern + "\nworst"]
            metric_rows = []
            metrics = [(None, "Complete callback µs"), ("state_cycles", "SP active µs"), ("job_cycles", "JP active µs"),
                ("axi_response_stall_cycles", "AXI response stall µs"), ("axi_address_stall_cycles", "AXI address stall µs"),
                ("axi_write_stall_cycles", "AXI write stall µs"), ("dram_queue_stall_cycles", "DRAM queue stall µs"),
                ("reads", "AXI read transactions"), ("writes", "AXI write transactions"), ("queue_peak", "Job queue peak")]
            for key, label in metrics:
                values = [label]
                for pattern, (run, peak) in sorted(worst.items()):
                    for row in [run["callbacks"][0], peak]:
                        v = elapsed_us(row) if key is None else row["measurement"].get(key)
                        if v is not None and key and key.endswith("_cycles"): v = v / 150
                        values.append(number(v))
                metric_rows.append(values)
            columns = len(headers) - 1
            table(fig, [.055, .255, .89, .365], headers, metric_rows, [.32] + [.68 / columns] * columns, size=9)
            notes = ["SP, JP and stall intervals may overlap; do not add them. These counters do not isolate allocator, reclamation and cache costs."]
            for pattern, (run, row) in sorted(worst.items()):
                index = row["callback"]
                following = f'; next callback {elapsed_us(run["callbacks"][index + 1]):.2f} µs' if index + 1 < len(run["callbacks"]) else ""
                final = run.get("metadata", {}).get("finalization", {}).get("native", {})
                notes.append(f'{pattern}: peak {row.get("event") or "steady"}{following}. Final memory idle={final.get("idle", "unreported")}; drain={final.get("final_drain_dram_ticks", "unreported")} DRAM ticks.')
            block(fig, .055, .20, notes, lines=6, size=8.5)
            pdf.savefig(fig); plt.close(fig)

        if hosts:
            fig = page("CPU and functional reference — secondary results", "Per-repeat ranges preserve host variability without pooling away unusually slow runs")
            host_rows = []
            def span(values):
                values = [v for v in values if v is not None]
                if not values: return "—"
                return number(min(values)) if min(values) == max(values) else f"{min(values):,.2f}–{max(values):,.2f}"
            for (backend, pattern), group in sorted(grouped.items()):
                if backend == "fpga-rtl": continue
                repeats = [stats([elapsed_us(row) for row in r["callbacks"]]) for r in group if r["status"] == "completed" and r.get("callbacks")]
                host_rows.append([backend + "\n" + pattern, status(group), f'{sum(s["count"] for s in repeats):,}',
                    *[span([s.get(key) for s in repeats]) for key in ("median_us", "p95_us", "p99_us", "max_us")], sum(s["budget_exceedances"] for s in repeats) if repeats else "—"])
            shown = host_rows[:12]
            table(fig, [.055, .515, .89, .32], ["Backend / pattern", "Run status", "Total N", "Median range µs", "p95 range µs", "p99 range µs", "Max range µs", "Over budget"], shown,
                  [.18, .16, .08, .13, .12, .12, .13, .08], size=8)
            fig.text(.055, .455, "Endpoint service outside processing — host wall time", fontsize=12, weight="bold")
            endpoint_rows = []
            for backend in sorted({r["backend"] for r in hosts}):
                values = [row["control_ns"] / 1000 for r in hosts if r["backend"] == backend for row in r["callbacks"]]
                s = stats(values)
                endpoint_rows.append([backend, f'{s["count"]:,}', number(s.get("median_us")), number(s.get("p99_us")), number(s.get("max_us"))])
            table(fig, [.055, .285, .89, .12], ["Backend", "N", "Median µs", "p99 µs", "Max µs"], endpoint_rows, size=9)
            block(fig, .055, .22, ["Each range spans individual completed repeats, not confidence bounds. Over-budget counts sum those repeats.",
                "Native CPU values are wall-clock observations and include scheduler effects. Rust FPGA reference values measure CPU execution of the functional model; they are not FPGA hardware estimates.",
                "Endpoint delivery/polling/snapshot work is measured separately from rendering. It is not added to fabric-cycle timing.",
                f"Showing {len(shown)} of {len(host_rows)} backend/pattern groups; all repeat-level statistics are retained in summary.json."], lines=8, size=9)
            pdf.savefig(fig); plt.close(fig)

        fig = page("Validation, capabilities and provenance", "Raw callback/event records and full per-repeat statistics remain available independently of this compact PDF")
        mandatory = [r for r in completed if r.get("fixture", {}).get("pattern") != "grab"]
        def verified(field, group):
            known = [r for r in group if field in r.get("coverage", {})]
            passed = sum(bool(r["coverage"][field]) for r in known)
            return f"{passed}/{len(group)} runs" + (f" ({len(group)-len(known)} unreported)" if len(known) != len(group) else "")
        coverage_rows = [
            ["Mandatory comparisons passed", "yes" if all_valid else "NO / incomplete"],
            ["Completed / unsupported / failed", f'{counts["completed"]} / {counts["unsupported"]} / {sum(v for k,v in counts.items() if k not in ("completed", "unsupported"))}'],
            ["Callback samples retained", f'{sum(len(r.get("callbacks", [])) for r in completed):,}'],
            ["Complete sample capture", verified("complete_samples", completed)],
            ["Exact stereo validation", verified("stereo_samples_checked", completed)],
            ["Cold block-boundary growth", verified("cold_block_boundary_verified", mandatory)],
            ["Take replacements observed", f'{sum(r.get("coverage", {}).get("replaced_takes", 0) for r in completed):,}'],
            ["Planned slots retired", f'{sum(r.get("coverage", {}).get("retired_plan_slots", 0) for r in completed):,}'],
        ]
        table(fig, [.055, .445, .43, .39], ["Check / coverage", "Result"], coverage_rows, [.61, .39], size=8.5)
        metadata = rtl[0].get("metadata", {}) if rtl else {}
        native = metadata.get("memory", {}).get("native", {})
        provenance = metadata.get("provenance", {})
        fixtures = sorted({(r.get("fixture", {}).get("pattern", "unknown"), r.get("fixture_sha256", "unreported")) for r in completed})
        provenance_lines = [f'Source: {manifest.get("revision", "unreported")}',
            f'Captured: {manifest.get("created_at", "unreported")}',
            f'Preset / repeats: {manifest.get("preset", "unreported")} / {manifest.get("repeats", "unreported")}',
            f'Rust: {manifest.get("rustc") or provenance.get("rustc", "unreported")}',
            f'Verilator: {provenance.get("verilator", "unreported")}',
            f'Memory model / seed: {metadata.get("memory_model", manifest.get("memory_model", "unreported"))} / {metadata.get("memory_seed", "unreported")}',
            'Memory configuration: ' + str(Path(native.get("configuration", {}).get("config", "unreported")).name),
            'Binary SHA256: ' + str(manifest.get("binary_sha256", "unreported"))]
        provenance_lines += [f'{name} fixture SHA256: {digest}' for name, digest in fixtures[:3]]
        block(fig, .525, .833, provenance_lines, width=66, lines=22, size=8)
        issues = Counter((r["backend"], r.get("fixture", {}).get("pattern", "unknown"), r["status"], r.get("error") or "") for r in runs if r["status"] != "completed")
        notes = [f'{count}× {backend}/{pattern}: {state}. {error}' for (backend, pattern, state, error), count in sorted(issues.items())]
        if not summary["complete"]: notes.insert(0, "Campaign capture is incomplete: some requested run slots are missing or the campaign was interrupted.")
        if not notes: notes = ["All requested operations completed; no unsupported or failed runs."]
        block(fig, .055, .355, notes, width=125, lines=8, size=8.5)
        block(fig, .055, .125, ["Full model/library/configuration hashes and environment metadata are retained in the run JSON and manifest.",
            "Regenerate: python3 scripts/benchmark_rt.py --report-only --output <campaign-directory>"], lines=3, size=8.5)
        pdf.savefig(fig); plt.close(fig)
        assert pdf.get_pagecount() <= 5, "compact benchmark report exceeded five pages"


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
