#!/usr/bin/env python3
"""Summarize the retained callback samples and RTL clock counts."""
import csv
import gzip
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def distribution(values):
    values = sorted(values)
    n = len(values)
    return {
        "samples": n,
        "mean": sum(values) / n,
        "min": values[0],
        "p50": values[n // 2],
        "p95": values[min(n - 1, n * 95 // 100)],
        "p99": values[min(n - 1, n * 99 // 100)],
        "max": values[-1],
    }


summary = {"cpu_us": {}, "rtl": {}, "rtl_ideal": {}}
for scenario in ("mixed", "midi"):
    summary["cpu_us"][scenario] = {}
    for backend in ("fpga", "production"):
        stem = f"cpu-{backend}-{scenario}"
        with gzip.open(ROOT / f"{stem}.csv.gz", "rt") as source:
            rows = list(csv.DictReader(source))
        metadata = json.loads((ROOT / f"{stem}.json").read_text())
        assert metadata["loops"] == 64 and metadata["sequence_frames"] == 1440000
        assert len(rows) == 27000
        result = distribution([int(row["elapsed_ns"]) / 1000 for row in rows])
        result["startup_us"] = metadata["start_playback_us"]
        result["startup_packets"] = metadata["start_playback_midi_packets"]
        summary["cpu_us"][scenario][backend] = result
    for group, prefix in (("rtl", "rtl"), ("rtl_ideal", "rtl-ideal")):
        rtl = json.loads((ROOT / f"{prefix}-{scenario}.json").read_text())
        assert rtl["loops"] == 64 and rtl["sequence_frames"] == 1440000
        assert rtl.get("ddr_added_delay_clocks", 7) == (0 if group == "rtl_ideal" else 7)
        steady = [m for m in rtl["measurements"] if m["kind"] == "steady"]
        assert [m["position"] for m in steady] == list(range(17 * 32, 33 * 32, 32))
        summary[group][scenario] = {
            "cycles": distribution([m["cycles"] for m in steady]),
            "us_at_100mhz": distribution([m["cycles"] / 100 for m in steady]),
            "startup": rtl["measurements"][0],
        }

(ROOT / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")

if __name__ == "__main__":
    import matplotlib.pyplot as plt
    from matplotlib.lines import Line2D
    from matplotlib.patches import Patch

    plt.rcParams.update({"font.size": 10, "svg.fonttype": "none"})
    fig, (cpu, rtl) = plt.subplots(1, 2, figsize=(12, 4.9))
    scenarios = ("mixed", "midi")
    names = ["64 stereo + MIDI loops", "64 MIDI-only loops"]
    colors = {"fpga": "#247a9a", "production": "#d58231"}
    for backend, offset in (("fpga", -0.17), ("production", 0.17)):
        values = [summary["cpu_us"][s][backend] for s in scenarios]
        y = [i + offset for i in range(2)]
        cpu.barh(y, [v["p50"] for v in values], height=0.28,
                 color=colors[backend], label="Rust FPGA reference" if backend == "fpga" else "Rust production")
        for pos, value in zip(y, values):
            cpu.text(value["p50"] + 7, pos, f'{value["p50"]:.1f}', va="center")
    cpu.set_title("CPU callback: medians")
    cpu.legend(loc="lower right", frameon=False)
    cpu.set_xlim(0, 720)
    rtl_colors = {"rtl": "#647f48", "rtl_ideal": "#a6b37c"}
    for group, offset in (("rtl", -0.17), ("rtl_ideal", 0.17)):
        for i, scenario in enumerate(scenarios):
            value = summary[group][scenario]["us_at_100mhz"]
            y = i + offset
            rtl.barh(y, value["p50"], height=0.28, color=rtl_colors[group])
            rtl.plot([value["min"], value["max"]], [y, y], color="black", marker="|")
            rtl.text(value["max"] * 1.02, y, f'{value["p50"]:,.1f}', va="center")
    rtl.set_xlim(0, max(summary[g][s]["us_at_100mhz"]["max"]
                       for s in scenarios for g in rtl_colors) * 1.3)
    rtl.set_title("RTL simulation: median bars, measured ranges")
    rtl.legend(handles=[Patch(color=rtl_colors["rtl"], label="DDR: 7 added clocks"),
                        Patch(color=rtl_colors["rtl_ideal"], label="Ideal DDR: 0 added clocks"),
                        Line2D([], [], color="#ac3841", linestyle="--",
                               label="Quantum duration: 666.7 µs")],
               loc="lower right", frameon=False, fontsize=9)
    for axis in (cpu, rtl):
        axis.set_yticks(range(2), names)
        axis.set_ylim(1.9, -0.55)
        axis.axvline(32 / 48000 * 1e6, color="#ac3841", linestyle="--", linewidth=1)
        axis.set_xlabel("Microseconds per 32-frame quantum")
        axis.grid(axis="x", alpha=0.15)
        axis.set_axisbelow(True)
        axis.spines[["top", "right"]].set_visible(False)
    fig.suptitle("64 active loops • 30 seconds of content each • 100 CC changes/s/loop")
    fig.text(0.5, 0.015, "Different horizontal scales. RTL assumes 100 MHz; the routed design has not met that clock.",
             ha="center", fontsize=9)
    fig.tight_layout(rect=(0, 0.04, 1, 0.94))
    fig.savefig(ROOT / "callback-times.svg")
    fig.savefig(ROOT / "callback-times.png", dpi=140)
