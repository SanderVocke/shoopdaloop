#!/usr/bin/env python3
"""Record loader FP side effects, VST3 implementation compatibility, and old no-FX routing."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys

spec=importlib.util.spec_from_file_location('runner',Path(__file__).with_name('run.py'))
runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--smoke',action='store_true')
    args=parser.parse_args();work=args.work.resolve();output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    release=work/'release-020/shoopdaloop-0.2.0.release-linux-x64.portable/lib'
    env=runner.environment(work);env['LD_LIBRARY_PATH']=str(release)+':'+env['LD_LIBRARY_PATH']
    loader=[]
    for library in [release/'libzita-resampler.so',release/'libshoopdaloop_backend.so',work/'Carla-reference/bin/libcarla_native-plugin.so',work/'Carla-full-reference/bin/libcarla_native-plugin.so']:
        if not library.exists():continue
        log=output/(library.parent.parent.name+'-'+library.name+'.loader.log')
        code=runner.command([work/'fp_loader_probe',library],log,env)
        loader.append({'library':str(library),'returncode':code,'sha256':runner.digest(library),'result':log.read_text().strip()})
    (output/'fp-library-initializers.json').write_text(json.dumps(loader,indent=2))
    if (work/'Carla-full-reference').exists():
        outcomes=[]
        for mode in ['default','force-native']:
            selected=runner.environment(work,recipe='full-reference',shim=False)
            if mode=='force-native':selected['CARLA_DO_NOT_USE_JUCE_FOR_VST3']='1'
            log=output/f'implementation-{mode}.log'
            code=runner.command([work/'abi_benchmark','native',work/'Carla-full-reference/bin/libcarla_native-plugin.so',work/'Carla-full-reference/bin/resources',work/'states/voyage.xml',1024,10,output/f'implementation-{mode}.csv'],log,selected)
            outcomes.append({'mode':mode,'returncode':code,'library_sha256':runner.digest(work/'Carla-full-reference/bin/libcarla_native-plugin.so')})
        (output/'implementation-control.json').write_text(json.dumps(outcomes,indent=2))
    env['SHOOP_BENCH_020_LIBRARY']=str(release/'libshoopdaloop_backend.so')
    code=runner.command([sys.executable,Path(__file__).with_name('legacy020.py'),'none',128,10 if args.smoke else 2000,output/'legacy-no-fx.csv'],output/'legacy-no-fx.log',env)
    (output/'legacy-no-fx-status.json').write_text(json.dumps({'returncode':code,'scope':'output correctness and aggregate backend profiling; no comparable per-block CPU timing'},indent=2))
    if code:raise SystemExit(code)


if __name__=='__main__':main()
