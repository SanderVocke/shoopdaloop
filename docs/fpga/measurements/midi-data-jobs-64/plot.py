#!/usr/bin/env python3
"""Plot nominal 100 MHz quantum times and the 32-frame audio budget."""
import json
from pathlib import Path

import matplotlib

matplotlib.use('Agg')
matplotlib.rcParams['svg.hashsalt'] = 'midi-data-jobs-64'
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
data = json.loads((HERE / 'summary.json').read_text())
fig, axes = plt.subplots(1, 2, figsize=(10, 4), constrained_layout=True)
for ax, key, title in zip(axes, ('median_us', 'startup_us'), ('Steady playback · median', 'Starting all 64 loops')):
    for delay, shift, label, color in (
        (0, -.19, 'Ideal DDR (zero added delay)', '#277da8'),
        (7, .19, 'DDR model (7 added clocks)', '#ef993e'),
    ):
        values = [data[f'{s}-{delay}'][key] for s in ('mixed', 'audio', 'midi')]
        bars = ax.bar([i + shift for i in range(3)], values, .36, color=color, label=label)
        ax.bar_label(bars, fmt='%.1f', fontsize=9, padding=3)
    ax.axhline(32 / 48000 * 1e6, color='#9d3742', linestyle='--', linewidth=1.2,
               label='32-frame budget (666.7 µs)')
    ax.set_xticks([0, 1, 2], ['Audio + MIDI', 'Audio only', 'MIDI only'])
    ax.set_title(title, loc='left', fontsize=11, weight='bold')
    ax.set_ylabel('µs per quantum at an assumed 100 MHz')
    ax.set_ylim(0, max(800, max(patch.get_height() for patch in ax.patches) * 1.2))
    ax.spines[['top', 'right']].set_visible(False)
    ax.set_axisbelow(True)
    ax.grid(axis='y', color='#e5e9ed')
handles, labels = axes[0].get_legend_handles_labels()
fig.legend(handles, labels, loc='outside lower center', ncol=3, frameon=False, fontsize=9)
fig.suptitle('64-loop event-only SP/JP RTL · clock-count simulation; physical timing is not closed', fontsize=12)
fig.savefig(HERE / 'quantum-times.svg', metadata={'Date': None})
