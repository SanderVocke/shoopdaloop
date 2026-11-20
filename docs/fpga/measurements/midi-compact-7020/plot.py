#!/usr/bin/env python3
"""Plot the resource comparison at the common, post-optimization stage."""

import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch

root = Path(__file__).resolve().parent
old = json.loads((root.parent / "midi-shared-jobs-7020/summary.json").read_text())[
    "runs"
]
new = json.loads((root / "summary.json").read_text())["runs"]
rows = [
    next(r for r in old if r["kind"] == kind and r["mhz"] == 100)
    for kind in ("audio", "shared")
]
rows.append(next(r for r in new if r["mhz"] == 100))
labels = ["Audio only", "Previous shared MIDI", "Compact shared MIDI"]
colors = ["#64748b", "#bf6238", "#197e86"]
metrics = [
    ("lut", "Logic (LUTs)", 53200),
    ("ff", "Registers", 106400),
    ("bram36", "On-chip RAM", 140),
    ("dsp", "DSP arithmetic", 220),
]
plt.rcParams.update(
    {
        "font.family": "DejaVu Sans",
        "font.size": 10,
        "svg.fonttype": "none",
        "svg.hashsalt": "midi-compact-7020",
    }
)
fig, axes = plt.subplots(1, 4, figsize=(10, 3.5), sharey=True)
for axis, (key, title, capacity) in zip(axes, metrics):
    values = [r["post_opt"][key] * 100 / capacity for r in rows]
    bars = axis.barh(range(3), values, color=colors, height=0.6)
    for bar, value in zip(bars, values):
        inside = value > 85
        axis.text(
            value - 3 if inside else value + 3,
            bar.get_y() + bar.get_height() / 2,
            f"{value:.1f}%",
            ha="right" if inside else "left",
            va="center",
            color="white" if inside else "black",
            fontsize=9,
        )
    axis.axvline(100, color="#9f2432", linestyle="--", linewidth=1)
    axis.set_xlim(0, 120)
    axis.set_xticks([0, 50, 100], ["0%", "50%", "100%"])
    axis.set_yticks([])
    axis.set_title(title, fontsize=10)
    axis.spines[["top", "left", "right"]].set_visible(False)
    axis.grid(axis="x", alpha=0.15)
    axis.set_axisbelow(True)
axes[0].invert_yaxis()
fig.suptitle(
    "Zynq-7020: replacing wide MIDI control with shared execution",
    x=0.06,
    ha="left",
    fontsize=13,
)
fig.text(
    0.06,
    0.86,
    "Same 64-stereo-loop profile; 100 MHz target; before placement.",
    fontsize=10,
)
fig.legend(
    [Patch(facecolor=c) for c in colors],
    labels,
    loc="lower center",
    ncol=3,
    frameon=False,
    bbox_to_anchor=(0.5, 0.025),
)
fig.subplots_adjust(left=0.06, right=0.98, bottom=0.22, top=0.74, wspace=0.25)
fig.savefig(root / "resources.svg", metadata={"Date": None})
