"""Compare saved host binaries on one CPU with fixed launchers and rotated order."""

import argparse, csv, hashlib, json, os, platform, statistics, subprocess, sys
from pathlib import Path
sys.path.insert(0, str(Path('scripts').resolve()))
from benchmark_audio_chunks import summarize
p = argparse.ArgumentParser()
p.add_argument('--candidates', nargs='+', required=True, help='name=host-binary')
p.add_argument('--launcher', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--buffer', type=int, default=32)
p.add_argument('--callbacks', type=int, default=256)
p.add_argument('--repeats', type=int, default=3)
p.add_argument('--scenarios', nargs='+', default=['idle','play','record','ramps'])
p.add_argument('--dense', action='store_true')
p.add_argument('--mode', default='compact')
p.add_argument('--voices',type=int,default=4)
a = p.parse_args()
os.sched_setaffinity(0, {min(os.sched_getaffinity(0))})
a.output.mkdir(parents=True, exist_ok=True)
candidates = [(name, Path(path).resolve()) for name,path in (c.split('=',1) for c in a.candidates)]
metadata = {'platform':platform.platform(), 'machine':platform.machine(), 'affinity':sorted(os.sched_getaffinity(0)), 'command':sys.argv,
            'hosts':{name:{'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()} for name,path in candidates},
            'rustc':subprocess.check_output(['rustc','--version'],text=True).strip()}
metadata['launcher']={'path':str(a.launcher.resolve()),'sha256':hashlib.sha256(a.launcher.read_bytes()).hexdigest()}
metadata['case']={'mode':a.mode,'buffer':a.buffer,'callbacks':a.callbacks,'repeats':a.repeats,'scenarios':a.scenarios,'dense':a.dense,'voices':a.voices,'warmup':0.1}
if (a.output/'machine.json').exists():
    previous=json.loads((a.output/'machine.json').read_text())
    assert previous['hosts']==metadata['hosts'], 'host binaries changed since previous runs'
    assert previous['launcher']==metadata['launcher'] and previous['case']==metadata['case'], 'benchmark parameters changed'
else:
    (a.output/'machine.json').write_text(json.dumps(metadata,indent=2)+'\n')
rows=[]
for repeat in range(a.repeats):
    order=candidates[repeat%len(candidates):]+candidates[:repeat%len(candidates)]
    for scenario in a.scenarios:
        for name,host in order:
            dest=a.output/f'{repeat}-{scenario}-{name}'
            command=[str(a.launcher.resolve()),'--host',str(host),'--mode',a.mode,'--scenario',scenario,'--callbacks',str(a.callbacks),'--buffer',str(a.buffer),'--warmup','0.1','--output',str(dest)]
            if a.dense:
                command += ['--tracks','16','--loops','128','--voices',str(a.voices)]
            row=None
            if dest.exists():
                try:
                    row=summarize(dest)
                    assert row['mode']==a.mode
                except Exception:
                    suffix=0
                    while dest.with_name(dest.name+f'.partial-{suffix}').exists(): suffix+=1
                    dest.rename(dest.with_name(dest.name+f'.partial-{suffix}'))
            if row is None:
                subprocess.run(command,check=True)
                row=summarize(dest)
            row.update(candidate=name,repeat=repeat)
            rows.append(row)
            (a.output/'results.json').write_text(json.dumps(rows,indent=2)+'\n')
for scenario in a.scenarios:
    for name,_ in candidates:
        selected=[r for r in rows if r['scenario']==scenario and r['candidate']==name]
        print(scenario,name,'p50',round(statistics.median(r['p50_us'] for r in selected),3),'p99',round(statistics.median(r['p99_us'] for r in selected),3),flush=True)

for name,path in candidates:
    assert hashlib.sha256(path.read_bytes()).hexdigest()==metadata['hosts'][name]['sha256']
