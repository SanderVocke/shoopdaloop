from pathlib import Path
import argparse
import json
import subprocess

parser = argparse.ArgumentParser(description='Summarize saved host timings and sampled CPU function tables.')
parser.add_argument('output', type=Path)
OUTPUT = parser.parse_args().output.resolve()


def functions(path):
    rows = []
    for line in path.read_text().splitlines():
        columns = line.split(None, 4)
        if len(columns) != 5:
            continue
        try:
            values = [float(value) for value in columns[:4]]
        except ValueError:
            continue
        rows.append(dict(exclusive_s=values[0], exclusive_pct=values[1],
                         inclusive_s=values[2], inclusive_pct=values[3], name=columns[4]))
    return rows


def analyze():
    runtime = json.loads((OUTPUT / 'runtime.json').read_text())
    results = []
    for run in runtime['runs']:
        if run['backend'] != 'fpga-rtl' or run['status'] != 'completed':
            continue
        base = OUTPUT / Path(run['file']).stem
        table = base.with_suffix('.functions.txt')
        if not table.exists():
            with table.open('w') as stream:
                subprocess.run(['gprofng', 'display', 'text', '-limit', '0', '-functions',
                                str(OUTPUT / run['sampling_experiment'])], stdout=stream,
                               stderr=subprocess.STDOUT, check=True)
        rows = functions(table)
        total = next(row['exclusive_s'] for row in rows if row['name'] == '<Total>')
        signal = sum(row['inclusive_s'] for row in rows if row['name'] in (
            '<rt_fpga_rtl::model::Model>::get', '<rt_fpga_rtl::model::Model>::set'))
        generated = sum(row['exclusive_s'] for row in rows if row['name'].startswith('Vbench_top'))
        dram = sum(row['inclusive_s'] for row in rows
                   if row['name'].startswith('<rt_fpga_testbench::timing::dram::Dram as ')
                   and row['name'].rsplit('::', 1)[-1] in ('advance', 'submit', 'ready'))
        assert total > 0 and signal + generated + dram <= total + .05
        categories = dict(signal_access=signal, generated_rtl=generated, dram_timing=dram,
                          other=max(0., total - signal - generated - dram))
        protocol = sum(row['inclusive_s'] for row in rows
                       if row['name'].startswith('<rt_fpga_testbench::protocol::')
                       and row['name'].endswith(' as rt_fpga_rtl::sim::Agent>::sample'))
        axi = sum(row['inclusive_s'] for row in rows if row['name'] in (
            '<rt_fpga_testbench::memory::axi::Axi>::drive',
            '<rt_fpga_testbench::memory::axi::Axi>::sample'))
        phases = run['host_profile']['phases']
        result = dict(backend=run['backend'], media=run['media'], pattern=run['pattern'],
                      file=run['file'], child_s=run['child_s'], cpu_s=total, categories_s=categories,
                      categories_pct={name: seconds / total * 100 for name, seconds in categories.items()},
                      fabric_s=run['fabric_s'], phases_s={name: value['elapsed_ns']/1e9 for name, value in phases.items()},
                      overlapping_diagnostics_s=dict(protocol_monitors=protocol, axi_agents=axi),
                      functions_file=table.name)
        results.append(result)
    totals = {name: sum(row['categories_s'][name] for row in results)
              for name in ('signal_access', 'generated_rtl', 'dram_timing', 'other')}
    cpu = sum(row['cpu_s'] for row in results)
    result = dict(complete=runtime['complete'], runs=results, cpu_s=cpu, categories_s=totals,
                  categories_pct={name: value/cpu*100 for name, value in totals.items()} if cpu else {},
                  overlapping_diagnostics_pct={name: sum(row['overlapping_diagnostics_s'][name] for row in results)/cpu*100
                                               for name in ('protocol_monitors', 'axi_agents')} if cpu else {})
    (OUTPUT / 'cpu-summary.json').write_text(json.dumps(result, indent=2) + '\n')
    print(f"Completed RTL profiles: {len(results)}; sampled CPU seconds: {cpu:.2f}")
    print('CPU percentages:', result['categories_pct'])
    return runtime, result


def timeline(runtime, cpu):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt

    rows = sorted((row for row in runtime['runs'] if row['backend'] == 'fpga-rtl' and row['status'] == 'completed'),
                  key=lambda row: row['started_s'])
    colors = {'audio': '#377eb8', 'midi': '#984ea3', 'audio-midi': '#269a6c'}
    fig = plt.figure(figsize=(12, 7.3))
    fig.suptitle('Where the smoke-suite host time goes', x=.08, ha='left', fontsize=18, weight='bold')
    ax = fig.add_axes([.18, .38, .73, .47])
    for i, row in enumerate(rows):
        start = row['started_s']/60
        duration = (row['finished_s']-row['started_s'])/60
        ax.barh(i, duration, left=start, color=colors[row['media']], height=.6)
        ax.text(start+duration+.35, i, f'{duration:.1f} min', va='center', fontsize=9)
    ax.set_yticks(range(len(rows)), [f"{row['media']} / {row['pattern']}" for row in rows], fontsize=9)
    ax.invert_yaxis()
    ax.set_xlim(0, runtime['elapsed_s']/60*1.15)
    ax.set_xlabel('Elapsed host wall time since suite start (minutes)')
    ax.grid(axis='x', alpha=.2)
    ax.set_axisbelow(True)
    ax.set_title('Three RTL workers; overlapping bars execute concurrently', loc='left', fontsize=11, pad=14)
    ax.axvline(runtime['elapsed_s']/60, color='#777777', linestyle='--', linewidth=.8)
    lower = fig.add_axes([.18, .19, .73, .08])
    labels = {'signal_access': 'Signal get/set + callees', 'generated_rtl': 'Generated RTL bodies',
              'dram_timing': 'DRAM timing + callees', 'other': 'Other host work'}
    shades = ['#e89a36', '#377eb8', '#269a6c', '#a9adb2']
    left = 0
    for (name, percentage), shade in zip(cpu['categories_pct'].items(), shades):
        lower.barh(0, percentage, left=left, color=shade, label=labels[name])
        if percentage > 3:
            lower.text(left+percentage/2, 0, f'{percentage:.1f}%', ha='center', va='center', fontsize=10)
        left += percentage
    lower.set_xlim(0, 100);lower.set_yticks([])
    lower.set_xlabel(f'Share of sampled CPU time, weighted across {len(cpu["runs"])} RTL workloads (%)')
    lower.legend(loc='upper center', bbox_to_anchor=(.5, -.9), ncol=4, frameon=False, fontsize=9)
    fig.text(.08, .035, 'Signal access and DRAM timing use disjoint API call trees; RTL uses exclusive function-body time.\n'
             'The categories do not overlap. Host sampling is separate from FPGA fabric cycles / 150 MHz.', fontsize=8.5)
    fig.savefig(OUTPUT / 'runtime-timeline.png', dpi=160)
    fig.savefig(OUTPUT / 'runtime-timeline.svg')
    plt.close(fig)


if __name__ == '__main__':
    runtime, cpu = analyze()
    if runtime['complete']:
        assert len(cpu['runs']) == 6
        timeline(runtime, cpu)
