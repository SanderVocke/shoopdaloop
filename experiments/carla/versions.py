#!/usr/bin/env python3
"""Compare the previous stable Carla with 2.5.10 using a fixed Voyage workload."""
import argparse
import csv
import importlib.util
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tarfile

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('runner',ROOT/'experiments/carla/run.py')
runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
OLD_SHA='226fb5d646b7541b82035080190e7440df1f92372fb798b4ad49289570e5ad81'


def prepare(work):
    archive=work/'carla-2.5.9.tar.gz'
    if not archive.exists():
        assert runner.command(['curl','-fL','https://github.com/falkTX/Carla/archive/refs/tags/v2.5.9.tar.gz','-o',archive],work/'version-download.log')==0
    assert runner.digest(archive)==OLD_SHA
    tree=work/'Carla-version-2.5.9'
    if not tree.exists():
        with tarfile.open(archive) as f:f.extractall(work,filter='data')
        (work/'Carla-2.5.9').rename(tree)
    env=os.environ.copy();deps=work/'deps'
    env.update(PATH=str(work/'pyqt/bin')+':'+env['PATH'],PYTHONPATH=str(work/'pyqt'),
               PKG_CONFIG_PATH=str(deps/'usr/lib/x86_64-linux-gnu/pkgconfig')+':'+str(deps/'usr/share/pkgconfig'),
               PKG_CONFIG_SYSROOT_DIR=str(deps),CFLAGS='-I'+str(deps/'usr/include'),CXXFLAGS='-I'+str(deps/'usr/include'),
               LDFLAGS='-L'+str(deps/'usr/lib/x86_64-linux-gnu'),LD_LIBRARY_PATH=str(deps/'usr/lib/x86_64-linux-gnu')+':'+env.get('LD_LIBRARY_PATH',''),
               XDG_CONFIG_HOME=str(work/'plugin-config'))
    assert runner.command(['pkg-config','--exists','x11','xcursor','xext','freetype2'],work/'version-feature-probe.log',env)==0
    assert runner.command(['make','clean'],work/'version-clean.log',env,cwd=tree)==0
    assert runner.command(['make','-j2','plugin'],work/'version-build.log',env,cwd=tree)==0
    build_harness(work)


def build_harness(work):
    assert runner.command(['g++','-O2','-std=c++17','-I',work/'Carla-reference/source/includes',ROOT/'experiments/carla/abi_benchmark.cpp','-ldl','-o',work/'version-benchmark'],work/'version-harness-build.log')==0


def trees(work):
    return {'2.5.9':work/'Carla-version-2.5.9','2.5.10':work/'Carla-full-reference'}


def invoke(work,version,interface,policy,state,blocks,prefix):
    tree=trees(work)[version];native=interface=='native'
    library=tree/'bin'/('libcarla_native-plugin.so' if native else 'carla.lv2/carla.so')
    resources=tree/'bin'/('resources' if native else 'carla.lv2')
    command=[work/'version-benchmark',interface,library,resources,state,1024,blocks,prefix.with_suffix('.csv')]
    env=runner.environment(work,recipe='full-reference',shim=False)
    if policy is None:env.pop('SHOOP_BENCH_FTZ_DAZ',None)
    else:env['SHOOP_BENCH_FTZ_DAZ']=str(policy)
    env.pop('CARLA_DO_NOT_USE_JUCE_FOR_VST3',None)
    code=runner.command(command,prefix.with_suffix('.log'),env)
    return code,command


def verify(work):
    directory=work/'version-verification';directory.mkdir(exist_ok=True)
    for version in trees(work):
        filename_version=version.replace('.', '_')
        for interface in ['native','lv2']:
            for workload in ['empty','gain','synthetic','voyage']:
                for policy in [None,0,1]:
                    prefix=directory/f'{filename_version}-{interface}-{workload}-fp{policy}'
                    code,_=invoke(work,version,interface,policy,work/f'states/{workload}.xml',10,prefix)
                    assert code==0,(prefix,prefix.with_suffix('.log').read_text())
            for name,altered in [('missing',(work/'states/synthetic.xml').read_text().replace(str(work/'workload.so'),str(work/'absent.so'))),('bypass',(work/'states/synthetic.xml').read_text().replace('<Active>Yes</Active>','<Active>No</Active>'))]:
                state=directory/f'{name}.xml';state.write_text(altered)
                code,_=invoke(work,version,interface,0,state,10,directory/f'{filename_version}-{interface}-{name}')
                assert code!=0,'invalid workload accepted'
    print('PASS: 48 positive version/interface/FP/workload checks and eight missing/bypass rejections',flush=True)


def measure(work,directory,repeats,blocks):
    directory.mkdir(parents=True,exist_ok=True)
    cases=[(version,interface,policy) for version in trees(work) for interface in ['native','lv2'] for policy in [0,1]]
    manifest={'purpose':'Previous stable version sensitivity; not a recovered user installation or macOS measurement',
              'shoop_020_published':'2025-11-09T13:26:33Z','carla_2510_published':'2025-07-31T18:05:01Z','carla_259_published':'2024-09-22T09:15:44Z',
              'old_archive_sha256':OLD_SHA,'current_archive_sha256':runner.ARCHIVE_SHA,'actual_dated_proxy':'2.5.10 (official release and historical Homebrew formula)',
              'prior_stable_proxy':'2.5.9','features':'same local full SDK/compiler, Release plugin target, JUCE VST3, no source patches',
              'sample_rate':48000,'frames':1024,'warmup_blocks':200,'measured_blocks':blocks,'repeats':repeats,'cadence':'continuous throughput',
              'fp_policy':'Explicit off/on on processing thread before each call, outside timed interval; before/after captured',
              'artifacts':{}}
    for version,tree in trees(work).items():
        for lib in ['libcarla_native-plugin.so','carla.lv2/carla.so']:
            manifest['artifacts'][str(tree/'bin'/lib)]=runner.digest(tree/'bin'/lib)
    for path in [work/'version-benchmark',work/'voyage-input.bin',work/'voyage/Voyage Voyage.vst3/Contents/x86_64-linux/Voyage Voyage.so',work/'states/voyage.xml',ROOT/'experiments/carla/abi_benchmark.cpp']:
        manifest['artifacts'][str(path)]=runner.digest(path)
    for tool in ['gcc','g++']:manifest[tool]=subprocess.check_output([tool,'--version'],text=True).splitlines()[0]
    (directory/'protocol.json').write_text(json.dumps(manifest,indent=2)+'\n')
    for repeat in range(repeats):
        for version,interface,policy in cases if repeat%2==0 else reversed(cases):
            case=f'version-{version}-{interface}-voyage-1024-fp{policy}'
            filename_case=case.replace('.', '_')
            prefix=directory/f'{filename_case}-{repeat:02d}'
            if prefix.with_suffix('.json').exists():continue
            before=Path('/sys/fs/cgroup/cpu.stat').read_text()
            code,command=invoke(work,version,interface,policy,work/'states/voyage.xml',blocks,prefix)
            validation='process failed' if code else 'passed'
            if code==0:
                rows=list(csv.DictReader(prefix.with_suffix('.csv').open()))
                mask=0x8040 if policy else 0
                valid=len(rows)==blocks and all(int(r['frames'])==1024 and int(r['sample_rate'])==48000 and int(r['mxcsr_before'])&0x8040==mask and int(r['mxcsr_after'])&0x8040==mask for r in rows)
                if not valid:code=1;validation='frame/rate/FP/sample-count validation failed'
            metadata={'case':case,'repeat':repeat,'status':'ok' if code==0 else 'failed','returncode':code,'command':[str(v) for v in command],
                      'validation':validation,'fp_policy':policy,'sample_rate':48000,'configured_frames':1024,
                      'affinity':sorted(os.sched_getaffinity(0)),'scheduler':os.sched_getscheduler(0),
                      'cpu_stat_before':before,'cpu_stat_after':Path('/sys/fs/cgroup/cpu.stat').read_text()}
            prefix.with_suffix('.json').write_text(json.dumps(metadata,indent=2)+'\n')
            print(case,repeat,metadata['status'],flush=True)
    rows=runner.summarize(directory)
    assert len(rows)==repeats*len(cases),'incomplete version/interface/policy/repetition matrix'


def analyze(work,directory):
    spec=importlib.util.spec_from_file_location('analysis',ROOT/'experiments/carla/analyze.py')
    analysis=importlib.util.module_from_spec(spec);spec.loader.exec_module(analysis)
    rows=runner.summarize(directory);by_case={}
    for row in rows:by_case.setdefault(row['case'],{})[row['repeat']]=row
    comparisons=[]
    for interface in ['native','lv2']:
        for policy in [0,1]:
            a=f'version-2.5.9-{interface}-voyage-1024-fp{policy}';b=f'version-2.5.10-{interface}-voyage-1024-fp{policy}'
            for metric in ['mean_caller_cpu_ns','mean_wall_ns','p99_wall_ns']:
                pairs=[by_case[b][i][metric]/by_case[a][i][metric] for i in sorted(by_case.get(a,{}).keys()&by_case.get(b,{}).keys()) if by_case[a][i]['status']=='ok' and by_case[b][i]['status']=='ok']
                record={'comparison':'Carla 2.5.10 / 2.5.9','interface':interface,'ftz_daz':policy,'metric':metric,'pairs':len(pairs),'paired_ratios':pairs}
                if len(pairs)>=2:
                    ratio,low,high=analysis.interval(pairs);threshold=.20 if 'wall' in metric else .05118612557789248
                    agrees=max(sum(v>1 for v in pairs),sum(v<1 for v in pairs))
                    record.update(ratio=ratio,ci95=[low,high],threshold=threshold,flag=len(pairs)>=8 and (low>1 or high<1) and abs(ratio-1)>threshold and agrees>=.8*len(pairs))
                comparisons.append(record)
    output=directory.parent/'output';output.mkdir(exist_ok=True)
    (output/'versions-comparisons.json').write_text(json.dumps(comparisons,indent=2)+'\n')
    shutil.copy2(directory/'summary.csv',output/'versions-runs.csv');shutil.copy2(directory/'protocol.json',output/'versions-protocol.json')
    lines=['# Previous stable Carla version comparison','','Ratio is 2.5.10 / 2.5.9; same host/plugin/input/FP policy. Linux source-build sensitivity, not a historical Mac binary reproduction.','', '| Interface | FTZ/DAZ | Metric | Pairs | Ratio [95% CI] | Screen |','|---|---:|---|---:|---|---|']
    for r in comparisons:
        effect=f"{r['ratio']:.3f} [{r['ci95'][0]:.3f}, {r['ci95'][1]:.3f}]" if 'ratio' in r else 'insufficient pairs'
        lines.append(f"| {r['interface']} | {r['ftz_daz']} | {r['metric']} | {r['pairs']} | {effect} | {r.get('flag','—')} |")
    (output/'VERSIONS.md').write_text('\n'.join(lines)+'\n')
    print('Analyzed',len(rows),'version run records',flush=True)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('action',choices=['prepare','verify','run','analyze','all'])
    parser.add_argument('--work',type=Path,required=True);parser.add_argument('--output',type=Path)
    parser.add_argument('--repeats',type=int,default=10);parser.add_argument('--blocks',type=int,default=2000)
    args=parser.parse_args();work=args.work.resolve();directory=(args.output or work/'versions').resolve()
    if args.action in ['prepare','all']:prepare(work)
    if args.action in ['verify','all']:verify(work)
    if args.action in ['run','all']:measure(work,directory,args.repeats,args.blocks)
    if args.action in ['analyze','all']:analyze(work,directory)


if __name__=='__main__':main()
