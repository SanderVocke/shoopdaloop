#!/usr/bin/env python3
"""Summarize independent repeated runs and bootstrap paired effect sizes."""
import argparse
import csv
import importlib.util
import json
import math
from pathlib import Path
import random
import statistics

spec=importlib.util.spec_from_file_location('runner',Path(__file__).with_name('run.py'))
runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)


def interval(values):
    rng=random.Random(137)
    log=[math.log(v) for v in values]
    samples=sorted(math.exp(statistics.mean(rng.choices(log,k=len(log)))) for _ in range(5000))
    return math.exp(statistics.mean(log)),samples[125],samples[4874]


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--data-root',type=Path);args=parser.parse_args()
    work=args.work;data=args.data_root or work;output=args.output;output.mkdir(parents=True,exist_ok=True)
    groups={};all_rows=[]
    for directory in ['results','controls','history','pin-control','contention','confirm','fp-context','voyage','voyage-paced']:
        source=data/directory
        if not source.exists(): continue
        rows=runner.summarize(source);all_rows.extend(dict(v,dataset=directory) for v in rows)
        groups[directory]={}
        for row in rows: groups[directory].setdefault(row['case'],{})[row['repeat']]=row
    comparisons=[]
    def compare(label,dataset,a,b,metric,noise=0,classification='CPU'):
        data=groups.get(dataset,{})
        baseline=data.get(a,{});candidate=data.get(b,{})
        pairs=[];used=[]
        for repeat in sorted(set(baseline)&set(candidate)):
            x,y=baseline[repeat],candidate[repeat]
            if x['status']!='ok' or y['status']!='ok' or metric not in x or metric not in y: continue
            # CPU attribution requires correct output and no missing processing samples.
            if classification=='CPU' and (x.get('invalid',0) or y.get('invalid',0) or x.get('misses',0) or y.get('misses',0)): continue
            if 'inner' in metric:
                if x.get('matched_calls') != x.get('blocks') or y.get('matched_calls') != y.get('blocks'): continue
            if x[metric]>0 and y[metric]>0: pairs.append(y[metric]/x[metric]);used.append(repeat)
        record={'comparison':label,'baseline':a,'candidate':b,'metric':metric,'pairs':len(pairs),'dataset':dataset,'type':classification}
        if len(pairs)>=2:
            estimate,low,high=interval(pairs)
            threshold=max(.05,3*noise) if classification=='CPU' else .20
            agrees=max(sum(v>1 for v in pairs),sum(v<1 for v in pairs))
            flag=len(pairs)>=8 and (low>1 or high<1) and abs(estimate-1)>threshold and agrees>=math.ceil(.8*len(pairs))
            record.update(ratio=estimate,ci95=[low,high],paired_ratios=pairs,run_ids=used,threshold=threshold,flag=flag)
        comparisons.append(record)
        return record
    controls=groups.get('controls',{})
    aa=[]
    for frames in [1024,128]:
        for workload in ['empty','synthetic']:
            prefix=f'controls-reference-native-{workload}-{frames}-continuous'
            other=f'controls-reference-native-repeat-{workload}-{frames}-continuous'
            a=controls.get(prefix,{});b=controls.get(other,{})
            for r in a.keys()&b.keys():
                if a[r].get('mean_caller_cpu_ns',0)>0 and b[r].get('mean_caller_cpu_ns',0)>0:
                    aa.append(abs(b[r]['mean_caller_cpu_ns']/a[r]['mean_caller_cpu_ns']-1))
            compare('A/A repeatability','controls',prefix,other,'mean_caller_cpu_ns')
            bare=f'controls-reference-native-bare-{workload}-{frames}-continuous'
            compare('Timing instrumentation overhead','controls',bare,prefix,'mean_caller_cpu_ns')
    noise=statistics.median(aa) if aa else 0
    for frames in [1024,256,128]:
        for workload in ['empty','gain','synthetic']:
            for mode in ['native','lv2']:
                a=f'abi-reference-{mode}-{workload}-{frames}-continuous';b=f'abi-packaged-{mode}-{workload}-{frames}-continuous'
                compare('Build-target recipe','results',a,b,'mean_caller_cpu_ns',noise)
            a=f'abi-reference-native-{workload}-{frames}-continuous';b=f'abi-reference-lv2-{workload}-{frames}-continuous'
            compare('LV2 versus native ABI','results',a,b,'mean_caller_cpu_ns',noise)
            a=f'engine-reference-native-{workload}-{frames}-continuous'
            for mode in ['bridge','subprocess','engine']:
                b=f'engine-reference-{mode}-{workload}-{frames}-continuous'
                compare('Current execution overhead','results',a,b,'mean_wall_ns',noise,'latency')
                compare('Current inner Carla CPU','results',a,b,'mean_inner_cpu_ns',noise)
            for mode in ['native','bridge','subprocess','engine']:
                a=f'history-pre-{mode}-{workload}-{frames}-continuous';b=f'history-post-{mode}-{workload}-{frames}-continuous'
                compare('Native switch: outer CPU','history',a,b,'mean_caller_cpu_ns',noise)
                compare('Native switch: completion latency','history',a,b,'p99_wall_ns',noise,'latency')
                # Cross-dataset comparison with exactly the same metric/schema.
                current=f'engine-reference-{mode}-{workload}-{frames}-continuous'
                if current in groups.get('results',{}): groups['history'][current]=groups['results'][current]
                compare('Post-switch versus current','history',b,current,'mean_wall_ns',noise,'latency')
            if frames==128:
                old=f'legacy-020-lv2-{workload}-128-continuous'
                pre=f'history-pre-native-{workload}-128-paced'
                for row in groups.get('history',{}).get(old,{}).values():
                    if 'mean_caller_cpu_ns' in row: row['comparable_inner_cpu_ns']=row['mean_caller_cpu_ns']
                for row in groups.get('history',{}).get(pre,{}).values():
                    if 'mean_inner_cpu_ns' in row: row['comparable_inner_cpu_ns']=row['mean_inner_cpu_ns']
                compare('0.2.0 versus pre-switch inner CPU (both paced)','history',old,pre,'comparable_inner_cpu_ns',noise)
    for mode in ['native','bridge','subprocess','engine']:
        case=f'engine-reference-{mode}-synthetic-1024-paced'
        if case in groups.get('contention',{}):
            groups.setdefault('pin-control',{})[case+'-loaded']=groups['contention'][case]
            compare('Controlled CPU contention','pin-control',case,case+'-loaded','p99_wall_ns',noise,'latency')
            compare('CPU cost under contention','pin-control',case,case+'-loaded','mean_inner_cpu_ns',noise)
            compare('Inner wall time under contention','pin-control',case,case+'-loaded','p99_inner_wall_ns',noise,'latency')
    meter={'transitions':0,'compatible':0,'positive_readings':0,'largest_percent':0}
    for path in (data/'results').glob('abi-*-native-*.csv'):
        frames=int(path.stem.split('-')[-3]); period=frames/48000
        previous=None
        for row in csv.DictReader(path.open()):
            if 'dsp_percent' not in row: continue
            value=float(row['dsp_percent']); outer=100*int(row['wall_ns'])/(period*1e9)
            meter['positive_readings']+=value>0
            meter['largest_percent']=max(meter['largest_percent'],value)
            if previous is not None:
                decay=previous*(1-period)
                compatible=abs(value-decay)<.0002 or (value>=previous and value<=outer+100*1.01e-6/period+.0002)
                meter['transitions']+=1;meter['compatible']+=compatible
            previous=value
    (output/'meter-audit.json').write_text(json.dumps(meter,indent=2))
    for metric in ['mean_caller_cpu_ns','mean_inner_cpu_ns','p99_wall_ns']:
        compare('Adjacent native-switch confirmation','confirm','history-pre-subprocess-synthetic-1024-continuous','history-post-subprocess-synthetic-1024-continuous',metric,noise,'latency' if metric=='p99_wall_ns' else 'CPU')
    for mode in ['native','bridge','subprocess','engine']:
        current=f'engine-reference-{mode}-voyage-1024-continuous'
        for revision in ['pre','post']:
            historical=f'history-{revision}-{mode}-voyage-1024-continuous'
            compare('Voyage historical versus current','voyage',historical,current,'mean_inner_cpu_ns',noise)
        compare('Voyage native switch','voyage',f'history-pre-{mode}-voyage-1024-continuous',f'history-post-{mode}-voyage-1024-continuous','mean_inner_cpu_ns',noise)
        compare('Voyage execution paths','voyage','engine-reference-native-voyage-1024-continuous',current,'mean_inner_cpu_ns',noise)
    old='legacy-020-lv2-voyage-1024-paced'
    for mode in ['native','bridge','subprocess','engine']:
        current=f'engine-reference-{mode}-voyage-1024-continuous'
        compare('Voyage 0.2.0 versus current (cadence differs)','voyage',old,current,'mean_inner_cpu_ns',noise)
    if 'voyage' in groups and 'voyage-paced' in groups:
        old='legacy-020-lv2-voyage-1024-paced'
        groups['voyage-paced'][old]=groups['voyage'].get(old,{})
        for case in ['history-pre-native-voyage-1024-paced','engine-reference-native-voyage-1024-paced','engine-reference-subprocess-voyage-1024-paced']:
            compare('Voyage matched cadence versus 0.2.0','voyage-paced',old,case,'mean_inner_cpu_ns',noise)
    fp=[]
    for name in ['denormal','voyage-tail-fp']:
        path=data/(name+'.csv')
        if not path.exists(): continue
        observations=list(csv.DictReader(path.open()));means={}
        for repeat in sorted(set(v['repeat'] for v in observations)):
            for mode in ['0','1']:
                values=[int(v['cpu_ns']) for v in observations if v['repeat']==repeat and v['ftz_daz']==mode]
                if values: means[(repeat,mode)]=statistics.mean(values)
        ratios=[means[(r,'0')]/means[(r,'1')] for r in sorted(set(v[0] for v in means)) if (r,'0') in means and (r,'1') in means]
        if ratios:
            ratio,low,high=interval(ratios)
            record={'experiment':name,'off_over_on_ratio':ratio,'ci95':[low,high],'pairs':len(ratios),'paired_ratios':ratios,
                    'mean_cpu_off_ns':statistics.mean(v for (r,m),v in means.items() if m=='0'),'mean_cpu_on_ns':statistics.mean(v for (r,m),v in means.items() if m=='1')}
            fp.append(record)
    (output/'floating-point.json').write_text(json.dumps(fp,indent=2))
    (output/'comparisons.json').write_text(json.dumps(comparisons,indent=2))
    (output/'all-runs.json').write_text(json.dumps(all_rows,indent=2))
    text=['# Carla experiment measurements','',f'A/A median absolute paired CPU ratio deviation: {noise:.2%}.',
          '', '| Comparison | Baseline → candidate | Metric | Pairs | Ratio [95% CI] | Flag |', '|---|---|---|---:|---|---|']
    for v in comparisons:
        if 'ratio' in v:
            text.append(f"| {v['comparison']} | {v['baseline']} → {v['candidate']} | {v['metric']} | {v['pairs']} | {v['ratio']:.3f} [{v['ci95'][0]:.3f}, {v['ci95'][1]:.3f}] | {v['flag']} |")
        else:
            text.append(f"| {v['comparison']} | {v['baseline']} → {v['candidate']} | {v['metric']} | {v['pairs']} | insufficient valid pairs | — |")
    text+=['','Flags are screening results under the predeclared thresholds, not causal diagnoses.',
           'CPU comparisons exclude runs with invalid output, deadline misses, or incomplete inner-call matching; latency summaries retain deadline/fallback effects.',
           'Old 0.2.0 samples are inner LV2 call timings under its internally paced dummy driver, not full callback timings.']
    (output/'MEASUREMENTS.md').write_text('\n'.join(text)+'\n')
    # Standalone, dependency-free SVG of current synthetic wall-time medians.
    bars=[]
    for frames in [1024,256,128]:
        for mode in ['native','bridge','subprocess','engine']:
            case=f'engine-reference-{mode}-synthetic-{frames}-continuous'
            values=[r['mean_wall_ns']/1000 for r in groups.get('results',{}).get(case,{}).values() if r['status']=='ok' and 'mean_wall_ns' in r]
            if values: bars.append((f'{frames} frames / {mode}',statistics.median(values)))
    maximum=max((v for _,v in bars),default=1)
    svg=['<svg xmlns="http://www.w3.org/2000/svg" width="900" height="570" viewBox="0 0 900 570">',
         '<rect width="900" height="570" fill="white"/><g font-family="sans-serif" font-size="15">',
         '<text x="24" y="30">Current synthetic workload: median per-run mean completion time (µs)</text>']
    for i,(name,value) in enumerate(bars):
        y=60+i*39
        svg += [f'<text x="24" y="{y+18}">{name}</text>',f'<rect x="260" y="{y}" width="{value/maximum*500:.2f}" height="26" fill="#2878aa"/>',f'<text x="{270+value/maximum*500:.2f}" y="{y+18}">{value:.1f}</text>']
    svg+=['</g></svg>'];(output/'completion-times.svg').write_text('\n'.join(svg))
    print(f'Wrote {len(all_rows)} run records and {len(comparisons)} comparisons to {output}')


if __name__=='__main__':
    main()
