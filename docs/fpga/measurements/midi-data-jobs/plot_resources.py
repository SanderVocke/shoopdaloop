#!/usr/bin/env python3
"""Render the 100 MHz post-optimization resource comparison."""
import json
from pathlib import Path

import matplotlib

matplotlib.use('Agg')
matplotlib.rcParams['svg.hashsalt'] = 'midi-data-jobs'
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
data = json.loads((HERE / 'summary.json').read_text())
before, after = (data['builds'][name]['post_opt'] for name in ('before-100', 'after-100'))
fig, axes = plt.subplots(2, 2, figsize=(9, 5.5), constrained_layout=True)
for ax, key, title in zip(
    axes.flat,
    ('luts', 'registers', 'bram_tiles', 'dsps'),
    ('Logic cells (LUTs)', 'Register bits', 'On-chip RAM tiles', 'DSP units'),
):
    totals = [item['total'][key] for item in (before, after)]
    midi = [item['midi'][key] for item in (before, after)]
    other = [total - part for total, part in zip(totals, midi)]
    ax.bar([0, 1], other, color='#bfc9d4', label='Shared audio/SP/JP and boundary')
    ax.bar([0, 1], midi, bottom=other, color='#277da8', label='MIDI functional unit')
    ax.set_xticks([0, 1], ['Previous compact service', 'Direct data jobs'])
    ax.set_title(title, loc='left', fontsize=11, weight='bold')
    ax.set_ylim(0, max(totals) * 1.22)
    ax.tick_params(axis='x', labelsize=9)
    for x, total in enumerate(totals):
        ax.annotate(f'{total:,g}', (x, total), xytext=(0, 5),
                    textcoords='offset points', ha='center', fontsize=10)
    ax.spines[['top', 'right']].set_visible(False)
    ax.set_axisbelow(True)
    ax.grid(axis='y', color='#e5e9ed')
handles, labels = axes[0, 0].get_legend_handles_labels()
fig.legend(handles, labels, loc='outside lower center', ncol=2, frameon=False, fontsize=9)
fig.suptitle('64 stereo + MIDI loops · Vivado 2021.1 · Zynq-7020 · post-optimization', fontsize=12)
fig.savefig(HERE / 'resource-impact.svg', metadata={'Date': None})
