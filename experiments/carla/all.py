#!/usr/bin/env python3
"""Execute the preparation, correctness checks, serial experiments and analysis."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
SCRIPTS=ROOT/'experiments/carla'


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--results-root',type=Path)
    parser.add_argument('--skip-prepare',action='store_true')
    parser.add_argument('--reuse-runtimes',action='store_true')
    parser.add_argument('--full-vst3',action='store_true')
    parser.add_argument('--previous-carla',action='store_true')
    parser.add_argument('--smoke',action='store_true')
    parser.add_argument('--apt-config',type=Path)
    parser.add_argument('--native-sysroot',type=Path)
    parser.add_argument('--current-target',type=Path)
    parser.add_argument('--pin-cpu',type=int,default=min(os.sched_getaffinity(0)))
    args=parser.parse_args()
    if args.previous_carla and not args.full_vst3:parser.error('--previous-carla requires --full-vst3')
    work=args.work.resolve();results=(args.results_root or work).resolve()
    work.mkdir(parents=True,exist_ok=True);results.mkdir(parents=True,exist_ok=True)
    def call(command,log,env=None):
        print('Running',log,flush=True)
        with (results/log).open('w') as f:
            subprocess.run([str(v) for v in command],cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
    if not args.skip_prepare:
        command=[sys.executable,SCRIPTS/'bootstrap.py','--work',work]
        for flag in ['reuse_runtimes','full_vst3']:
            if getattr(args,flag): command.append('--'+flag.replace('_','-'))
        for flag in ['apt_config','native_sysroot','current_target']:
            if getattr(args,flag): command+=['--'+flag.replace('_','-'),getattr(args,flag)]
        call(command,'bootstrap.log')
    if args.previous_carla and not args.skip_prepare:
        call([sys.executable,SCRIPTS/'versions.py','prepare','--work',work],'version-prepare.log')
    call([sys.executable,SCRIPTS/'verify.py',work],'verification.log')
    if args.previous_carla:call([sys.executable,SCRIPTS/'versions.py','verify','--work',work],'version-verify.log')
    for suite,directory,extra in [('all','results',[]),('controls','controls',[]),('history','history',[]),
                                  ('confirm','confirm',[]),('fp-context','fp-context',['--repeats',1,'--blocks',10,'--paced-blocks',10])]:
        command=[sys.executable,SCRIPTS/'run.py','run','--work',work,'--output',results/directory,'--suite',suite]+extra
        if args.smoke: command+=['--repeats',1,'--blocks',10,'--paced-blocks',10]
        call(command,directory+'-progress.log')
    if args.full_vst3:
        command=[sys.executable,SCRIPTS/'run.py','run','--work',work,'--output',results/'voyage','--suite','voyage','--blocks',200,'--paced-blocks',200]
        if args.smoke:command+=['--repeats',1,'--blocks',10,'--paced-blocks',10]
        call(command,'voyage-progress.log')
        paced=[sys.executable,SCRIPTS/'run.py','run','--work',work,'--output',results/'voyage-paced','--suite','voyage-paced','--blocks',200,'--paced-blocks',200]
        if args.smoke:paced+=['--repeats',1,'--blocks',10,'--paced-blocks',10]
        call(paced,'voyage-paced-progress.log')
        env=os.environ.copy();env['LD_LIBRARY_PATH']=str(work/'deps/usr/lib/x86_64-linux-gnu')+':'+env.get('LD_LIBRARY_PATH','');env['XDG_CONFIG_HOME']=str(work/'plugin-config')
        if args.smoke:env['SHOOP_BENCH_SMOKE']='1'
        call([work/'denormal_probe',work/'Carla-reference/bin/libcarla_native-plugin.so',work/'Carla-reference/bin/resources',work/'denormal_workload.so',results/'denormal.csv'],'denormal.log',env)
        call([work/'denormal_probe',work/'Carla-full-reference/bin/libcarla_native-plugin.so',work/'Carla-full-reference/bin/resources',work/'denormal_workload.so',results/'voyage-tail-fp.csv',work/'states/voyage.xml'],'voyage-tail-fp.log',env)
    for directory,load in [('pin-control',False),('contention',True)]:
        command=[sys.executable,SCRIPTS/'run.py','run','--work',work,'--output',results/directory,'--suite','contention','--pin-cpu',args.pin_cpu]
        if load:command+=['--load']
        if args.smoke:command+=['--repeats',1,'--blocks',10,'--paced-blocks',10]
        call(command,directory+'-progress.log')
    if args.full_vst3:
        supplementary=[sys.executable,SCRIPTS/'supplementary.py','--work',work,'--output',results/'output']
        if args.smoke:supplementary+=['--smoke']
        call(supplementary,'supplementary.log')
    if args.previous_carla:
        version_run=[sys.executable,SCRIPTS/'versions.py','run','--work',work,'--output',results/'versions']
        if args.smoke:version_run+=['--repeats',1,'--blocks',10]
        call(version_run,'version-progress.log')
        call([sys.executable,SCRIPTS/'versions.py','analyze','--work',work,'--output',results/'versions'],'version-analysis.log')
    call([sys.executable,SCRIPTS/'analyze.py','--work',work,'--data-root',results,'--output',results/'output'],'analysis.log')
    print('Finished; measurement failures remain explicit in summaries.',flush=True)


if __name__=='__main__':
    main()
