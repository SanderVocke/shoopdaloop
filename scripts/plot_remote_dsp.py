#!/usr/bin/env python3
"""Plot output deadline misses from the recorded localhost experiment archive."""
from __future__ import annotations
import argparse
import gzip
import json
import os
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    cache = Path(__file__).resolve().parents[1] / "work" / "plot-cache"
    os.environ.setdefault("MPLCONFIGDIR", str(cache / "matplotlib"))
    os.environ.setdefault("XDG_CACHE_HOME", str(cache))
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.colors import LogNorm
    from matplotlib.cm import ScalarMappable
    from matplotlib.patches import Rectangle, Patch

    with gzip.open(args.results, "rt", encoding="utf-8") as handle:
        rows = [json.loads(line) for line in handle]
    packets = [8, 16, 32, 64, 128]
    allowances = [1, 2, 4, 8, 16]
    norm = LogNorm(vmin=1e-5, vmax=1)
    colors = plt.get_cmap("YlOrRd")
    figure, axes = plt.subplots(1, 4, figsize=(13, 4.2), layout="constrained")
    for axis, multiplier in zip(axes, [1, 2, 4, 8]):
        for y, packet in enumerate(packets):
            for x, allowance in enumerate(allowances):
                runs = [row for row in rows if row["suite"] == "baseline"
                        and row["requested"]["P"] == packet and row["requested"]["k"] == multiplier
                        and row["requested"]["N"] == allowance and row["status"] == "measured"]
                if not runs:
                    fill, label = "#dddddd", "ref"
                else:
                    fraction = sum(row["late"] + row["missing"] for row in runs) / sum(row["measured_packets"] for row in runs)
                    fill = "#d9f0d3" if fraction == 0 else colors(norm(max(fraction, 1e-5)))
                    label = "0" if fraction == 0 else f"{100*fraction:.2g}%"
                axis.add_patch(Rectangle((x-0.5, y-0.5), 1, 1, facecolor=fill, edgecolor="white"))
                axis.text(x, y, label, ha="center", va="center", fontsize=8)
        axis.set(xlim=(-0.5, 4.5), ylim=(4.5, -0.5), xticks=range(5), yticks=range(5),
                 xticklabels=allowances, yticklabels=packets, title=f"Block multiplier k={multiplier}", xlabel="Allowance N (packet periods)")
        axis.tick_params(length=0)
    axes[0].set_ylabel("Packet frames P")
    figure.suptitle("Delay-copy DSP: output deadline misses across three baseline repetitions", fontsize=13)
    figure.colorbar(ScalarMappable(norm=norm, cmap=colors), ax=axes, shrink=0.7, label="Output deadline miss fraction")
    figure.legend(handles=[Patch(facecolor="#d9f0d3", label="Zero observed output misses"), Patch(facecolor="#dddddd", label="Structurally refused")],
                  loc="outside lower center", ncols=2, frameon=False)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(args.output, metadata={"Date": None})
    plt.close(figure)


if __name__ == "__main__":
    main()
