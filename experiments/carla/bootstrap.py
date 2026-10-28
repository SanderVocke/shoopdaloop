#!/usr/bin/env python3
"""Prepare pinned sources, local dependencies, runtimes and historical adapters."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[2]
SHA = 'ae2835b12081f7271a6b0b25d34b87d36b022c40370028ca4a10f90fcedfa661'


def checked(args, log, cwd=None, env=None):
    with log.open('w') as f:
        subprocess.run([str(v) for v in args], cwd=cwd, env=env, stdout=f,
                       stderr=subprocess.STDOUT, check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--apt-config', type=Path)
    parser.add_argument('--reuse-runtimes', action='store_true')
    parser.add_argument('--current-target', type=Path)
    args = parser.parse_args()
    work = args.work.resolve(); work.mkdir(parents=True, exist_ok=True)
    archive = work / 'carla.tar.gz'
    if not archive.exists():
        checked(['curl','-fL','https://github.com/falkTX/Carla/archive/refs/tags/v2.5.10.tar.gz','-o',archive], work/'download.log')
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == SHA
    debs = work/'debs'; debs.mkdir(exist_ok=True)
    deps = work/'deps'; deps.mkdir(exist_ok=True)
    apt = ['apt-get'] + (['-c',args.apt_config] if args.apt_config else [])
    packages = ['liblo-dev','liblo7','liblilv-dev','liblilv-0-0','libserd-dev','libsord-dev',
                'libsratom-dev','libserd-0-0','libsord-0-0','libsratom-0-0','lv2-dev','libzix-dev','libzix-0-0']
    if not (deps/'usr/lib/x86_64-linux-gnu/pkgconfig/zix-0.pc').exists():
        checked(apt+['download']+packages,work/'dependencies.log',cwd=debs)
        for package in debs.glob('*.deb'):
            checked(['dpkg-deb','-x',package,deps],work/'extract.log')
    if not (work/'pyqt/bin/pyuic5').exists():
        checked([sys.executable,'-m','pip','install','--target',work/'pyqt','PyQt5==5.15.11'],work/'pyqt-install.log')
    env=os.environ.copy()
    env['PATH']=str(work/'pyqt/bin')+':'+env['PATH']
    env['PYTHONPATH']=str(work/'pyqt')
    env['PKG_CONFIG_PATH']=str(deps/'usr/lib/x86_64-linux-gnu/pkgconfig')
    env['PKG_CONFIG_SYSROOT_DIR']=str(deps)
    env['LD_LIBRARY_PATH']=str(deps/'usr/lib/x86_64-linux-gnu')+':'+env.get('LD_LIBRARY_PATH','')
    env['LIBRARY_PATH']=str(deps/'usr/lib/x86_64-linux-gnu')+':'+env.get('LIBRARY_PATH','')
    for recipe in ['reference','packaged']:
        tree=work/f'Carla-{recipe}'
        if not tree.exists():
            with tarfile.open(archive) as f:
                f.extractall(work,filter='data')
            (work/'Carla-2.5.10').rename(tree)
        if not args.reuse_runtimes:
            checked(['make','clean'],work/f'{recipe}-clean.log',cwd=tree,env=env)
            targets=['plugin']+(['frontend'] if recipe=='packaged' else [])
            checked(['make','-j2']+targets,work/f'{recipe}-build.log',cwd=tree,env=env)
    # Runtime selection and native dependencies for Cargo are independent of Carla build flags.
    build_env=os.environ.copy()
    build_env['LIBRARY_PATH']=env['LIBRARY_PATH']; build_env['LD_LIBRARY_PATH']=env['LD_LIBRARY_PATH']
    build_env['PKG_CONFIG_PATH']=env['PKG_CONFIG_PATH']+':'+build_env.get('PKG_CONFIG_PATH','')
    build_env['PKG_CONFIG_SYSROOT_DIR']=str(deps)
    build_env['CARGO_TARGET_DIR']=str(work/'target-historical')
    for revision, commit, feature in [('pre','9b169b5','lv2'),('post','0cec63c','carla')]:
        tree=work/revision
        if not tree.exists():
            checked(['git','clone','--shared','--no-checkout',ROOT,tree],work/f'{revision}-clone.log')
            checked(['git','checkout','--detach',commit],work/f'{revision}-checkout.log',cwd=tree)
            checked(['git','apply',ROOT/f'experiments/carla/adapters/{revision}.patch'],work/f'{revision}-adapter.log',cwd=tree)
        checked(['cargo','build','--locked','--release','-p','shoop_engine','--features',feature,'--example','carla_regression_benchmark'],work/f'{revision}-build.log',cwd=tree,env=build_env)
        shutil.copy2(work/'target-historical/release/examples/carla_regression_benchmark',work/f'{revision}_benchmark')
    release=work/'release-020.tar.gz'
    if not release.exists():
        checked(['curl','-fL','https://github.com/SanderVocke/shoopdaloop/releases/download/v0.2.0/shoopdaloop-0.2.0.release-linux-x64.portable.tar.gz','-o',release],work/'release-download.log')
    if not (work/'release-020').exists():
        with tarfile.open(release) as f:
            f.extractall(work/'release-020',filter='data')
    bundle=work/'legacy-lv2/carla.lv2'
    if not bundle.exists():
        shutil.copytree(work/'Carla-reference/bin/carla.lv2',bundle,symlinks=True)
        (bundle/'carla.so').unlink(); (bundle/'carla.so').symlink_to(work/'native_timing.so')
        # Resolve resources without adding a non-bundle child to LV2_PATH.
        (bundle/'resources').unlink(); (bundle/'resources').symlink_to(work/'Carla-reference/bin/resources')
    current_env=os.environ.copy(); current_env['CARGO_TARGET_DIR']=str(args.current_target or work/'target-current')
    checked(['cargo','build','--locked','--release','-p','shoop_engine','--features','carla','--example','carla_regression_benchmark'],work/'current-build.log',cwd=ROOT,env=current_env)
    shutil.copy2(Path(current_env['CARGO_TARGET_DIR'])/'release/examples/carla_regression_benchmark',work/'carla_regression_benchmark')
    checked([sys.executable,ROOT/'experiments/carla/run.py','prepare','--work',work],work/'prepare.log')
    versions={p.name:subprocess.check_output(['dpkg-deb','--field',str(p),'Package','Version'],text=True) for p in debs.glob('*.deb')}
    (work/'dependency-manifest.json').write_text(json.dumps(versions,indent=2))


if __name__=='__main__':
    main()
