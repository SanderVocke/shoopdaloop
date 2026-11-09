#!/usr/bin/env python3
"""Summarize weighted user-CPU samples from profile_audio_chunks.py."""
import argparse
from collections import Counter
import csv
import gzip
import json
import os
from pathlib import Path
import re
import subprocess


CATEGORIES = (
    'Loop planning and state', 'Audio storage and indexing', 'MIDI buffers and history',
    'Reference processing', 'Graph nodes, mixing and meters', 'Loop capture coordination',
    'Runtime scheduling and dispatch', 'State publication', 'Host wrapper and other',
    'Compact graph execution (mixed)', 'Unattributed',
)


def category(symbol):
    owner = symbol.lstrip('<')
    if owner.startswith(('shoop_loops::resolver::', 'shoop_loops::runtime::LoopPlanner',
                         'shoop_loops::runtime::planner::')):
        return CATEGORIES[0]
    if owner.startswith('rt_runtime::resource::'):
        if any(name in owner.split('>')[0] for name in ('midi', 'Midi')):
            return CATEGORIES[2]
        return CATEGORIES[1]
    if owner.startswith(('rt_runtime::midi', 'rt_runtime::buffer::Midi')):
        return CATEGORIES[2]
    if owner.startswith(('rt_runtime::audio_refs::', 'rt_runtime::executor::referenced_audio::')):
        return CATEGORIES[3]
    if owner.startswith('rt_runtime::executor::'):
        if 'run_graph_compact' in owner:
            return CATEGORIES[9]
        if any(name in symbol for name in ('::run_graph_references', '::reference_', '::forward_')):
            return CATEGORIES[3]
        if owner.startswith(('rt_runtime::executor::mix', 'rt_runtime::executor::route_offset')):
            return CATEGORIES[4]
        return CATEGORIES[6]
    if owner.startswith('shoop_loops::runtime::shared::'):
        return CATEGORIES[5]
    if owner.startswith(('shoop_rt_engine::node::', 'shoop_rt_engine::processors::',
                         'shoop_host_processors::', 'shoop_dsp::', 'rt_buffered::',
                         'shoop_loops::runtime::processor::')):
        return CATEGORIES[4]
    if owner.startswith(('shoop_rt_engine::cells::', 'shoop_rt_engine::monitoring::')):
        return CATEGORIES[7]
    if owner.startswith(('rt_runtime::schedule::', 'rt_runtime::transport::',
                         'rt_runtime::host_buffer::', 'rt_runtime::planner::')):
        return CATEGORIES[6]
    if owner.startswith(('shoop_rt_host_native::', 'shoop_rt_engine::render::',
                         'rt_runtime::profiling::', 'rt_profiling::')):
        return CATEGORIES[8]
    return None


def parse_stacks(text):
    samples = []
    period, frames = None, []
    for line in text.splitlines() + ['']:
        header = re.fullmatch(r'\S+\s+\d+\s+(\d+)\s*', line)
        frame = re.fullmatch(r'\s+[0-9a-f]+\s+(.+)\s+\(([^()]*)\)', line)
        if header:
            if period is not None:
                samples.append((period, frames))
            period, frames = int(header[1]), []
        elif frame and period is not None:
            frames.append((frame[1], frame[2]))
        elif not line.strip() and period is not None:
            samples.append((period, frames))
            period, frames = None, []
    if not samples or any(period <= 0 or not frames for period, frames in samples):
        raise ValueError('empty or malformed sampled stacks')
    return samples


def analyze(args, path, env):
    common = ['-i', str(path)]
    if args.symfs:
        common += ['--symfs', str(args.symfs)]
    report = subprocess.run(
        [str(args.perf), 'report', '--stdio', '--no-children', '--call-graph', 'none',
         '--percent-limit', '0', *common], env=env, text=True, capture_output=True, check=True)
    script = subprocess.run(
        [str(args.perf), 'script', '--no-demangle', '-F', 'comm,tid,period,ip,sym,dso', *common],
        env=env, text=True, capture_output=True, check=True)
    samples = parse_stacks(script.stdout)
    displayed_samples = re.search(r'# Samples:\s+(\S+)', report.stdout)[1]
    event_count = int(re.search(r'# Event count \(approx\.\):\s+(\d+)', report.stdout)[1])
    lost = int(re.search(r'# Total Lost Samples:\s+(\d+)', report.stdout)[1])
    if displayed_samples.isdigit():
        assert len(samples) == int(displayed_samples), (path, len(samples), displayed_samples)
    assert sum(period for period, _ in samples) == event_count
    assert lost == 0, (path, lost)
    symbols = sorted({symbol for _, frames in samples for symbol, _ in frames})
    demangled = subprocess.run(['c++filt', '-s', 'rust'], input='\n'.join(symbols) + '\n',
                               text=True, capture_output=True, check=True).stdout.splitlines()
    assert len(symbols) == len(demangled)
    names = {symbol: re.sub(r'\[[0-9a-f]{6,}\]', '', name)
             for symbol, name in zip(symbols, demangled)}
    weights = Counter()
    leaf_weights = Counter()
    folded = Counter()
    folded_counts = Counter()
    copy_weight = planner_copy_weight = unknown_leaf = without_callback = 0
    for period, raw_frames in samples:
        frames = [names[symbol] for symbol, _ in raw_frames]
        group = next((group for symbol in frames if (group := category(symbol))), CATEGORIES[-1])
        weights[group] += period
        leaf_weights[frames[0]] += period
        folded[tuple(frames)] += period
        folded_counts[tuple(frames)] += 1
        if raw_frames[0][0] == '[unknown]':
            unknown_leaf += period
        if not any(category(symbol) for symbol in frames):
            without_callback += period
        if any(name in frames[0] for name in ('memcpy', 'memmove', 'memset', 'bzero')):
            copy_weight += period
            if any(category(symbol) == CATEGORIES[0] for symbol in frames):
                planner_copy_weight += period
    metadata = json.loads(path.with_name('sampling.json').read_text())
    callbacks = path.with_name('callbacks.csv')
    samples_enabled = metadata.get('sample_processing', True)
    if callbacks.exists():
        header = callbacks.read_text().splitlines()[0]
        samples_enabled = 'sample_processing=0' not in header
    assert metadata.get('sample_processing', samples_enabled) == samples_enabled
    result = {
        'mode': metadata['mode'], 'scenario': metadata['scenario'], 'samples': len(samples),
        'sample_processing': samples_enabled,
        'lost_samples': lost, 'total_period_ns': event_count,
        'category_percent': {name: 100 * weights[name] / event_count for name in CATEGORIES},
        'unknown_leaf_percent': 100 * unknown_leaf / event_count,
        'unattributed_percent': 100 * without_callback / event_count,
        'bulk_memory_percent': 100 * copy_weight / event_count,
        'planner_bulk_memory_percent': 100 * planner_copy_weight / event_count,
        'top_leaf_symbols': [{'symbol': symbol, 'percent': 100 * weight / event_count}
                             for symbol, weight in leaf_weights.most_common(30)],
    }
    destination = args.output / metadata['scenario'] / metadata['mode']
    destination.mkdir(parents=True, exist_ok=True)
    (destination / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    with gzip.open(destination / 'stacks.jsonl.gz', 'wt') as output:
        for frames, weight in folded.items():
            output.write(json.dumps({'count': folded_counts[frames], 'period_ns': weight,
                                     'frames_leaf_first': frames}) + '\n')
    (destination / 'perf-report.txt').write_text(report.stdout)
    (destination / 'symbolization.log').write_text(report.stderr + script.stderr)
    print(f'{metadata["scenario"]}/{metadata["mode"]}: {len(samples)} samples, '
          f'{result["unattributed_percent"]:.1f}% unattributed', flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--perf', type=Path, required=True)
    parser.add_argument('--perf-library-path', type=Path)
    parser.add_argument('--symfs', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env['DEBUGINFOD_URLS'] = ''
    if args.perf_library_path:
        env['LD_LIBRARY_PATH'] = str(args.perf_library_path) + ':' + env.get('LD_LIBRARY_PATH', '')
    results = [analyze(args, path, env) for path in sorted(args.input.glob('*/*/perf.data'))]
    if not results:
        parser.error('no collected perf.data files found')
    (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    with (args.output / 'categories.csv').open('w') as output:
        writer = csv.writer(output)
        writer.writerow(['scenario', 'mode', 'samples', *CATEGORIES])
        for result in results:
            writer.writerow([result['scenario'], result['mode'], result['samples'],
                             *[result['category_percent'][name] for name in CATEGORIES]])


if __name__ == '__main__':
    main()
