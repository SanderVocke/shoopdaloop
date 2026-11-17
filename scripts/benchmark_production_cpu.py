#!/usr/bin/env python3
"""Measure production float/fixed processing and optional paired app baselines."""
import argparse
import csv
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from rt_backend_testbench.cases import benchmark64,step
ROOT=Path(__file__).resolve().parents[1]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,default=ROOT/'work/production-cpu')
    p.add_argument('--baseline',type=Path)
    p.add_argument('--quanta',type=int,default=1024)
    p.add_argument('--repeats',type=int,default=3)
    p.add_argument('--cpu',type=int)
    args=p.parse_args()
    if args.quanta<32 or args.repeats<1:p.error('use >=32 quanta and positive repeats')
    if args.cpu is not None:os.sched_setaffinity(0,{args.cpu})
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    def stats(values):
        x=sorted(values);return dict(median_us=statistics.median(x)/1000,p99_us=x[int((len(x)-1)*.99)]/1000,max_us=max(x)/1000)
    scenario=benchmark64();scenario['steps']=scenario['steps'][:1]+[step(ports=0) for _ in range(args.quanta-1)]
    text=json.dumps(scenario);(out/'numeric.scenario.json').write_text(text+'\n')
    rows=[]
    for repeat in range(args.repeats):
        for numeric in (['float','q824'] if repeat%2==0 else ['q824','float']):
            run=subprocess.run([str(ROOT/'target/release/examples/cpu'),numeric],input=text,text=True,capture_output=True,check=True)
            value=json.loads(run.stdout);(out/f'{numeric}-{repeat}.json').write_text(json.dumps(value)+'\n')
            rows.append(dict(kind='numeric',numeric=numeric,repeat=repeat,**stats(value['nanoseconds'][16:]),
                allocations=value['callback_allocations'],reallocations=value['callback_reallocations'],deallocations=value['callback_deallocations']))
    if args.baseline:
        for phase in ('play','record-play','record','clear'):
            for repeat in range(args.repeats):
                pair=[('before',args.baseline),('after',ROOT/'target/release/examples/cpu_benchmark')]
                for label,binary in pair[::1 if repeat%2==0 else -1]:
                    path=out/f'{label}-{phase}-{repeat}.csv'
                    with path.open('w') as output:subprocess.run([str(binary.resolve()),'--mode','current','--voices','4','--callbacks','1024','--phase',phase],stdout=output,check=True)
                    data=list(csv.DictReader(path.open()))
                    rows.append(dict(kind='app',version=label,phase=phase,repeat=repeat,**stats([int(x['elapsed_ns']) for x in data]),
                        **{k:sum(int(x[k]) for x in data) for k in ('allocations','reallocations','deallocations')}))
    report=dict(revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        quantum=32,numeric_loops=64,numeric_warmup_quanta=16,affinity=sorted(os.sched_getaffinity(0)),
        app_callback_frames=128,app_voices=4,rows=rows)
    (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
if __name__=='__main__':main()
