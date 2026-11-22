"""Plot measured long-run cache alternatives; no timing-qualified deadline claim."""
import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

HERE = Path(__file__).resolve().parent
ROWS = [
    ('C1', 'c1-extended-100-summary.json'),
    ('Four-way', 's2-fourway-extended-100-summary.json'),
    ('Banked', 's2-banked-extended-100-summary.json'),
    ('Full leaf', 's2-leaf-extended-100-summary.json'),
    ('One block', 's2-block-extended-100-summary.json'),
    ('Directory path', 's2-path-extended-100-summary.json'),
]
fig, ax = plt.subplots(figsize=(9, 5.4), layout='constrained')
for i, (label, name) in enumerate(ROWS):
    result = json.loads((HERE/name).read_text())
    assert not result['stages']['route']['timing_met']
    benchmark = next(b for b in result['benchmarks'] if b['cycles']['steady']['count'] == 1024)
    cycles = benchmark['cycles']['steady']['max']/1000
    slices = result['stages']['route']['slices']
    color = '#087e8b' if label == 'Directory path' else '#475569'
    ax.scatter(slices, cycles, color=color, s=65, zorder=3)
    ax.annotate(label, (slices, cycles), xytext=(7, 7), textcoords='offset points', fontsize=10)
ax.axhline(100_000_000*32/48_000/1000, color='#c17b16', linestyle='--', linewidth=1.3,
           label='Nominal 100 MHz quantum budget')
ax.set(xlabel='Routed slices', ylabel='Maximum observed steady quantum (thousand clocks)',
       title='Cache footprint and boundary latency', xlim=(11870, 12790), ylim=(54, 103))
ax.grid(alpha=.18)
ax.legend(loc='upper right', frameon=False)
fig.text(.01, -.035, 'Same 1,024-quantum mixed/state-on/delay-7 workload. All six builds fail setup timing.\n'
         'Observed maxima cover two leaf spans; deeper boundaries have separate controller checks.', fontsize=9)
for suffix in ['png', 'pdf']:
    fig.savefig(HERE/f'cache-tradeoffs.{suffix}', dpi=160, bbox_inches='tight')
