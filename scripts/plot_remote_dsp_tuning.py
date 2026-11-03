#!/usr/bin/env python3
"""Compare the matched 60-second scheduling experiments from their archive."""
from __future__ import annotations
import argparse
import gzip
import json
import os
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--results', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    cache = Path(__file__).resolve().parents[1] / 'work' / 'plot-cache'
    os.environ.setdefault('MPLCONFIGDIR', str(cache / 'matplotlib'))
    os.environ.setdefault('XDG_CACHE_HOME', str(cache))
    import matplotlib
    matplotlib.use('Agg')
    matplotlib.rcParams['svg.hashsalt'] = 'remote-dsp-tuning'
    import matplotlib.pyplot as plt
    from matplotlib.patches import Patch
    with gzip.open(args.results, 'rt', encoding='utf-8') as handle:
        rows = [json.loads(line) for line in handle if line.strip()]
    rows = [row for row in rows if row['suite'] == 'followup']
    configs = sorted({tuple(row['requested'][key] for key in ['P', 'k', 'N']) for row in rows})
    figure, axes = plt.subplots(1, 2, figsize=(13, 5.5), layout='constrained', sharey=True)
    colors = ['#4477aa', '#aa3377']
    for y, config in enumerate(configs):
        group = [row for row in rows if tuple(row['requested'][key] for key in ['P', 'k', 'N']) == config]
        if len(group) != 2 or any(row['status'] != 'measured' for row in group):
            raise ValueError(f'incomplete measured pair: {config}')
        group.sort(key=lambda row: row['mode'] != 'yield-free')
        for index, row in enumerate(group):
            values = [row['late'] + row['missing'], row['server']['concealed_packets']]
            position = y + (-0.18 if index == 0 else 0.18)
            for axis, value in zip(axes, values):
                axis.barh(position, value, height=0.32, color=colors[index])
                axis.plot(value, position, 'o', color=colors[index], markersize=3)
                axis.annotate(str(value), (value, position), xytext=(5, 0), textcoords='offset points', va='center', fontsize=8)
    maxima = [max(row['late'] + row['missing'] for row in rows),
              max(row['server']['concealed_packets'] for row in rows)]
    for axis, maximum in zip(axes, maxima):
        axis.set_xscale('symlog', linthresh=1)
        axis.set_xlim(0, max(2, maximum * 1.9))
        axis.grid(axis='x', alpha=0.2)
        axis.set_axisbelow(True)
        axis.set_yticks(range(len(configs)))
        axis.set_yticklabels([f'P={p}, k={k}, N={n} ({n*p/48:.2f} ms)' for p, k, n in configs])
        axis.set_ylim(len(configs)-0.5, -0.5)
    axes[0].set_title('Output deadline misses (late + missing)')
    axes[1].set_title('Input packets replaced by the gap policy')
    axes[0].set_xlabel('Count in nominal 60-second measured interval')
    axes[1].set_xlabel('Count across stream, including 0.5-second warm-up')
    figure.suptitle('Scheduling comparisons: one fresh baseline and one selected alternative per configuration', fontsize=12)
    figure.legend(handles=[Patch(color=colors[0], label='Yielding, unpinned baseline'), Patch(color=colors[1], label='Alternative selected from short runs (varies by row)')], loc='outside lower center', ncols=2, frameon=False)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(args.output, metadata={'Date': None})
    if args.output.suffix.lower() == '.svg':
        args.output.write_text('\n'.join(line.rstrip() for line in args.output.read_text().splitlines()) + '\n')
    plt.close(figure)


if __name__ == '__main__':
    main()
