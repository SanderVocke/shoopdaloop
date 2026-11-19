#!/usr/bin/env python3
"""Render the resource comparison from collected Vivado reports."""
import json
from pathlib import Path
import sys

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
data = json.loads((root/'results.json').read_text())
matplotlib.rcParams.update({'svg.hashsalt': 'midi-vivado-7020', 'font.size': 10})
fig, axes = plt.subplots(1, 2, figsize=(10.5, 3.7), sharey=True, layout='constrained')
keys = ['luts', 'registers', 'bram36_equivalent', 'dsps']
limits = np.array([53200, 106400, 140, 220])
x = np.arange(4)
colors = ['#235789', '#E09F3E']
for axis, mhz in zip(axes, [100, 150]):
    for j, version in enumerate(['before', 'after']):
        row = next(d for d in data if d['mhz'] == mhz and d['version'] == version)
        counts = row['stages']['opt']['total']
        values = 100*np.array([counts[k] for k in keys])/limits
        bars = axis.bar(x+(j-.5)*.36, values, width=.34, color=colors[j],
                        label=['Audio only', 'Audio + MIDI'][j])
        axis.bar_label(bars, labels=[f'{v:.1f}%' for v in values], padding=3, fontsize=9)
    axis.axhline(100, color='#AC3931', linewidth=1, linestyle='--')
    axis.set_ylim(0, 109)
    axis.set_xticks(x, ['Logic\n(LUTs)', 'Registers', 'Memory\n(BRAM)', 'Multiply\n(DSP)'])
    axis.set_title(f'{mhz} MHz target', fontsize=12)
    axis.grid(axis='y', alpha=.18)
    axis.set_axisbelow(True)
    for side in ['top', 'right']:
        axis.spines[side].set_visible(False)
axes[0].set_ylabel('Share of Zynq-7020 resources (%)')
axes[0].legend(loc='upper right', frameon=False, fontsize=9)
fig.suptitle('Resource demand after logic optimization', fontsize=14)
fig.savefig(root/'resources.svg', metadata={'Date': None}, bbox_inches='tight')
