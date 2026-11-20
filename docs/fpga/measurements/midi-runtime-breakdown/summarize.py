#!/usr/bin/env python3
"""Summarize disjoint runtime categories and retain the independent job view."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
source = json.loads((ROOT / "profile.json").read_text())
results = []
for m in source["measurements"]:
    if "profile" not in m:
        continue
    p = m["profile"]
    jobs = p["job_cycles"]
    categories = p["categories"]
    assert sum(categories.values()) == m["cycles"] == p["cycles"]
    assert not categories.get("restore_or_cleanup", 0)
    groups = {
        "Preparing playback windows": jobs["31"],
        "Other per-loop servicing": sum(jobs.get(str(op), 0) for op in [25, 26, 28, 29, 30, 33]),
        "Finding state trackers": categories["tracker_directory_walk"],
    }
    groups["Other work"] = m["cycles"] - sum(groups.values())
    assert groups["Other work"] >= 0
    requests = sum(r["reads"] + r["writes"] for r in p["memory_regions"].values())
    results.append({"position":m["position"],"messages":m["midi_packets"],
                    "cycles":m["cycles"],"groups":groups,"categories":categories,
                    "job_cycles":jobs,"memory_requests":requests,
                    "memory_regions":p["memory_regions"],
                    "comparison_pairs":p["max_comparison_pairs"]})
results.sort(key=lambda r: r["messages"])
(ROOT / "summary.json").write_text(json.dumps(results, indent=2) + "\n")

if __name__ == "__main__":
    import matplotlib.pyplot as plt
    plt.rcParams.update({"svg.fonttype":"none", "font.size":10})
    colors = ["#247a9a", "#63a1b7", "#d58231", "#b8b8b8"]
    fig, axis = plt.subplots(figsize=(10, 3.8))
    for i, r in enumerate(results):
        left = 0
        for index, ((label, cycles), color) in enumerate(zip(r["groups"].items(), colors)):
            width = cycles / 100
            axis.barh(i, width, left=left, height=0.48, color=color,
                      label=label if i == 0 else None)
            if width > 800:
                axis.text(left + width / 2, i, f'{cycles / r["cycles"]:.1%}',
                          ha="center", va="center", color="white" if index == 0 else "black")
            left += width
        axis.text(left + 50, i, f'{left:,.0f} µs', va="center")
    axis.axvline(32 / 48000 * 1e6, linestyle="--", color="#ac3841",
                 label="Quantum budget: 667 µs")
    axis.set_yticks(range(len(results)), [f'{r["messages"]} emitted messages' for r in results])
    axis.set_ylim(1.6, -0.6)
    axis.set_xlim(0, 6600)
    axis.set_xlabel("Microseconds at an assumed 100 MHz; ideal DDR, AXI retained")
    axis.set_title("64 active MIDI loops: where the quantum time goes")
    axis.spines[["top", "right"]].set_visible(False)
    axis.legend(loc="upper center", bbox_to_anchor=(0.5, -0.21), ncol=2, frameon=False)
    fig.tight_layout()
    fig.savefig(ROOT / "breakdown.svg", bbox_inches="tight")
    fig.savefig(ROOT / "breakdown.png", bbox_inches="tight", dpi=140)
