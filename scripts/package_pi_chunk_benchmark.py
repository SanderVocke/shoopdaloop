#!/usr/bin/env python3
"""Package an existing ARM64 release host and benchmark launcher for a Pi 4."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import zipfile

from benchmark_audio_chunks import MODES


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True, type=Path)
    parser.add_argument('--launcher', required=True, type=Path)
    parser.add_argument('--runtime-root', required=True, type=Path)
    parser.add_argument('--binary-source-sha', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    args.output.mkdir(parents=True, exist_ok=False)
    binary_dir = args.output / 'bin'
    libraries = binary_dir / 'lib'
    libraries.mkdir(parents=True)
    for source, name in ((args.host, 'shoop_host'), (args.launcher, 'chunk_benchmark')):
        header = source.read_bytes()[:20]
        if header[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', header, 18)[0] != 183:
            raise ValueError(f'{source} is not an ARM64 Linux ELF executable')
        shutil.copy2(source, binary_dir / name)
        (binary_dir / name).chmod(0o755)
    runtime_names = ('libasound.so.2', 'libstdc++.so.6', 'libgcc_s.so.1',
                     'libjack.so.0', 'libdb-5.3.so')
    for name in runtime_names:
        candidates = [args.runtime_root / prefix / name for prefix in
                      ('usr/lib/aarch64-linux-gnu', 'lib/aarch64-linux-gnu', 'usr/aarch64-linux-gnu/lib')]
        source = next((path for path in candidates if path.exists()), None)
        if source is None:
            raise FileNotFoundError(f'missing ARM64 runtime library {name}')
        shutil.copy2(source, libraries / name)
    scripts = args.output / 'scripts'
    scripts.mkdir()
    for name in ('benchmark_audio_chunks.py', 'profile_audio_chunks.py',
                 'analyze_audio_chunk_profiles.py', 'collect_audio_chunk_results.py'):
        shutil.copy2(repo / 'scripts' / name, scripts / name)
    for name, script, extra in (
            ('benchmark.sh', 'benchmark_audio_chunks.py', ''),
            ('profile.sh', 'profile_audio_chunks.py', ' --perf "$(command -v perf)"')):
        wrapper = args.output / name
        wrapper.write_text('#!/bin/sh\nset -eu\n'
                           'bundle_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\n'
                           f'exec python3 "$bundle_dir/scripts/{script}" '
                           '--host "$bundle_dir/bin/shoop_host" '
                           '--launcher "$bundle_dir/bin/chunk_benchmark"'
                           + extra + ' "$@"\n')
        wrapper.chmod(0o755)
    shutil.copy2(repo / 'docs/design/chunks/PI_BENCHMARKS.md', args.output / 'README.md')
    docs = args.output / 'docs'
    docs.mkdir()
    for name in ('PROTOTYPE.md', 'RESULTS.md', 'PROFILING.md', 'FOURTH_RESULTS.md'):
        shutil.copy2(repo / 'docs/design/chunks' / name, docs / name)
    shutil.copy2(repo / 'docs/design/chunks/audio-only-host.json', args.output / 'audio-only-host.json')
    notices = args.output / 'licenses'
    notices.mkdir()
    shutil.copy2(repo / 'LICENSE', notices / 'shoopdaloop-LICENSE')
    for name in ('libasound2', 'libstdc++6', 'libgcc-s1', 'libjack-jackd2-0', 'libdb5.3'):
        path = args.runtime_root / 'usr/share/doc' / name / 'copyright'
        if path.exists():
            shutil.copy2(path, notices / f'{name}-copyright')
    for path in (repo / 'third_party').glob('*/LICENSE*'):
        shutil.copy2(path, notices / f'{path.parent.name}-{path.name}')
    for name in ('GPL-3', 'LGPL-2.1', 'LGPL-3'):
        path = Path('/usr/share/common-licenses') / name
        if path.exists():
            shutil.copy2(path, notices / name)
    binary_source = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', args.binary_source_sha], text=True).strip()
    info = {
        'binary_source_sha': binary_source,
        'helper_source_sha': subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
        'source_url': f'https://github.com/SanderVocke/shoopdaloop/tree/{binary_source}',
        'upstream_sha': subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'origin/feat/backend-replacement'], text=True).strip(),
        'target': 'aarch64-unknown-linux-gnu', 'cpu': 'cortex-a72',
        'disabled_target_features': ['aes', 'sha2'],
        'cflags': '-mcpu=cortex-a72+nocrypto',
        'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
        'native_toolchain': 'Debian Bookworm GCC 12.2.0; glibc 2.36 sysroot',
        'build': 'cargo build --locked --release -p shoop_host --features testing --bin shoop_host --example chunk_benchmark --target aarch64-unknown-linux-gnu',
        'rustflags': '-D warnings -C target-cpu=cortex-a72 -C target-feature=-aes,-sha2 -C force-unwind-tables=yes -C link-arg=-Wl,-rpath,$ORIGIN/lib',
        'linker': 'aarch64-linux-gnu-gcc --sysroot=<Debian Bookworm cross-toolchain root>',
        'features': ['testing', 'cpal', 'jack', 'builtin-fx', 'oxisynth'],
        'requires': '64-bit Linux ARM64, glibc >= 2.36, Python 3, taskset; optional perf/c++filt',
        'binary_validation': 'ARM64 ELF checks; build warnings denied; emulated actual-app smoke tested separately',
        'modes': list(MODES),
        'benchmark_audio_only': 'MIDI device ports, history/index storage, loop/source state buffers and processing omitted in every mode',
        'files_sha256': {str(p.relative_to(args.output)): digest(p)
                         for p in sorted(args.output.rglob('*')) if p.is_file()},
    }
    (args.output / 'BUILD_INFO.json').write_text(json.dumps(info, indent=2) + '\n')
    paths = [p for p in sorted(args.output.rglob('*')) if p.is_file()]
    (args.output / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.relative_to(args.output)}\n' for p in paths))
    archive_path = args.output.with_suffix('.zip')
    with zipfile.ZipFile(archive_path, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(args.output.rglob('*')):
            if path.is_file():
                archive.write(path, Path(args.output.name) / path.relative_to(args.output))
    print(json.dumps({'archive': str(archive_path), 'bytes': archive_path.stat().st_size,
                      'sha256': digest(archive_path)}, indent=2))


if __name__ == '__main__':
    main()
