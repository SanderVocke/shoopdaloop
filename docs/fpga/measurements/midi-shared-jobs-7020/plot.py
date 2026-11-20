#!/usr/bin/env python3
"""Draw the comparable-stage 100 MHz resource chart from summary.json."""
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Patch

root = Path(__file__).resolve().parent
runs = json.loads((root / 'summary.json').read_text())['runs']
rows = [next(r for r in runs if r['kind'] == kind and r['mhz'] == 100)
        for kind in ['audio', 'previous', 'shared']]
labels = ['Audio only', 'Audio + earlier MIDI', 'Audio + shared MIDI']
colors = ['#64748b', '#c36a32', '#177f88']
metrics = [('lut', 'Logic (LUTs)', 53200), ('ff', 'Registers', 106400),
           ('bram36', 'On-chip RAM', 140), ('dsp', 'DSP arithmetic', 220)]
plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 10, 'svg.fonttype': 'none', 'svg.hashsalt': 'midi-shared-jobs-7020'})
fig, axes = plt.subplots(1, 4, figsize=(10, 3.5), sharey=True)
for axis, (key, title, capacity) in zip(axes, metrics):
    values = [r['post_opt'][key] * 100 / capacity for r in rows]
    bars = axis.barh(range(3), values, color=colors, height=.6)
    for bar, value in zip(bars, values):
        inside = value > 85
        axis.text(value - 3 if inside else value + 3, bar.get_y() + bar.get_height()/2,
                  f'{value:.1f}%', ha='right' if inside else 'left', va='center',
                  color='white' if inside else 'black', fontsize=9)
    axis.axvline(100, color='#9f2432', linestyle='--', linewidth=1)
    axis.set_xlim(0, 120)
    axis.set_xticks([0, 50, 100], ['0%', '50%', '100%'])
    axis.set_yticks([])
    axis.set_title(title, fontsize=10)
    axis.spines[['top', 'left', 'right']].set_visible(False)
    axis.grid(axis='x', alpha=.15)
    axis.set_axisbelow(True)
axes[0].invert_yaxis()
fig.suptitle('Zynq-7020 resources: 100 MHz target', x=.06, ha='left', fontsize=14)
fig.text(.06, .86, 'After logic optimization, before placement. Percent of device capacity.', fontsize=10)
fig.legend([Patch(facecolor=c) for c in colors], labels, loc='lower center', ncol=3,
           frameon=False, bbox_to_anchor=(.5, .025))
fig.subplots_adjust(left=.06, right=.98, bottom=.22, top=.74, wspace=.25)
fig.savefig(root / 'resources.svg', metadata={'Date': None})
