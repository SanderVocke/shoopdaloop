#!/usr/bin/env python3
"""Export compact review data and a separate archive of full raw measurements."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import shutil
import tarfile


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    work=args.work.resolve();output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    generated=work/'output'
    for name in ['MEASUREMENTS.md','comparisons.json','floating-point.json','meter-audit.json',
                 'completion-times.svg','VERSIONS.md','versions-comparisons.json','versions-runs.csv','versions-protocol.json','versions-selection.json','macos-upstream-audit.json','build-protocol.json','implementation-control.json','fp-library-initializers.json','zita-fp-initializer.txt']:
        if (generated/name).exists():shutil.copy2(generated/name,output/name)
    rows=json.loads((generated/'all-runs.json').read_text())
    with (output/'raw-runs.csv').open('w') as f:
        writer=csv.DictWriter(f,fieldnames=sorted(set().union(*(r.keys() for r in rows))))
        writer.writeheader();writer.writerows(rows)
    for source,name in [(work/'manifest.json','extended-manifest.json'),(work/'frozen-core/manifest.json','core-manifest.json'),(work/'dependency-manifest.json','dependency-manifest.json')]:
        if source.exists():shutil.copy2(source,output/name)
    identities={}
    sources=['release-020.tar.gz','release-020/shoopdaloop-0.2.0.release-linux-x64.portable/lib/libshoopdaloop_backend.so',
             'release-020/shoopdaloop-0.2.0.release-linux-x64.portable/lib/libzita-resampler.so',
             'voyage/Voyage Voyage.vst3/Contents/x86_64-linux/Voyage Voyage.so','voyage.zip','voyage-input.bin',
             'denormal_workload.so','denormal_probe','fp_loader_probe','optional_vst3_probe','carla-2.5.9.tar.gz','version-benchmark','Carla-version-2.5.9/bin/libcarla_native-plugin.so','Carla-version-2.5.9/bin/carla.lv2/carla.so']
    for name in sources:
        path=work/name
        if path.exists():identities[name]=hashlib.sha256(path.read_bytes()).hexdigest()
    (output/'supplementary-identities.json').write_text(json.dumps(identities,indent=2)+'\n')
    archive=generated/'raw-results.tar.gz'
    directories=['results','controls','history','confirm','fp-context','voyage','voyage-paced','pin-control','contention','versions']
    with tarfile.open(archive,'w:gz') as tar:
        for directory in directories:
            if (work/directory).exists():tar.add(work/directory,arcname=directory)
        for name in ['denormal.csv','voyage-tail-fp.csv','manifest.json','dependency-manifest.json','frozen-core/manifest.json',
                     'voyage-crash.txt','version-build.log','version-verify.log','version-smoke.log','output/versions-selection.json','output/macos-upstream-audit.json','output/implementation-control.json','output/fp-library-initializers.json',
                     'legacy-no-fx.csv','legacy-no-fx.profile.json','legacy-no-fx.log','output/build-protocol.json',
                     'output/legacy-no-fx.csv','output/legacy-no-fx.profile.json','output/legacy-no-fx.log','output/legacy-no-fx-status.json']:
            if (work/name).exists():tar.add(work/name,arcname=name)
    digest=hashlib.sha256(archive.read_bytes()).hexdigest()
    (output/'raw-archive.json').write_text(json.dumps({'path':str(archive),'sha256':digest,'bytes':archive.stat().st_size,
        'scope':'Full per-block CSV, timing captures, commands/status and logs; binaries excluded. Archive retained in workspace, not committed.'},indent=2)+'\n')
    print('Exported',len(rows),'run summaries; raw archive:',archive)


if __name__=='__main__':main()
