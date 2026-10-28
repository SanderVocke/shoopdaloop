#!/usr/bin/env python3
"""Stage complete Intel Carla apps using the bundled-runtime Catalina recipe."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import stat
import struct
import subprocess

from check_macos_compatibility import inspect_macho, select_slice, verify_payloads

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / 'third_party/carla/runtime-lock.json'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sections(path):
    data = select_slice(path.read_bytes(), 'x86_64')
    result = {}
    position = 32
    for _ in range(struct.unpack_from('<I', data, 16)[0]):
        command, size = struct.unpack_from('<II', data, position)
        if command == 0x19:
            for index in range(struct.unpack_from('<I', data, position + 64)[0]):
                entry = position + 72 + index * 80
                name = data[entry:entry + 16].split(b'\0', 1)[0].decode()
                segment = data[entry + 16:entry + 32].split(b'\0', 1)[0].decode()
                length = struct.unpack_from('<Q', data, entry + 40)[0]
                offset = struct.unpack_from('<I', data, entry + 48)[0]
                if offset and offset + length <= len(data):
                    result[segment + '/' + name] = hashlib.sha256(data[offset:offset + length]).hexdigest()
        position += size
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--search-root', type=Path, required=True)
    parser.add_argument('--dependency-dir', type=Path, required=True)
    parser.add_argument('--normalized-runtime', type=Path, required=True)
    parser.add_argument('--license', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != 'Darwin' or platform.machine() != 'x86_64':
        raise RuntimeError('Packaging requires an Intel macOS runner')
    output = args.output.resolve()
    if output.exists():
        raise RuntimeError('Use a fresh staging directory')
    output.mkdir(parents=True)
    lock = json.loads(LOCK.read_text())
    apps = []
    for name in ['Carla.app', 'Carla-Control.app']:
        found = list(args.search_root.rglob(name))
        if len(found) != 1:
            raise RuntimeError(f'Expected exactly one complete {name}, found {found}')
        destination = output / name
        shutil.copytree(found[0], destination, symlinks=True)
        for path in destination.rglob('*'):
            if not path.is_symlink():
                path.chmod(path.stat().st_mode | stat.S_IWUSR)
        apps.append(destination)
    replacements = []
    removed = []
    for app in apps:
        binary_dir = app / 'Contents/MacOS'
        for directory in [binary_dir, binary_dir / 'resources']:
            if not directory.is_dir():
                continue
            for name in ['libcrypto.3.dylib', 'liblzma.5.dylib']:
                target = directory / name
                shutil.copy2(args.dependency_dir / 'lib' / name, target)
                replacements.append(str(target.relative_to(output)))
        for name in ['QtDesigner.so', 'QtHelp.so']:
            for path in app.rglob(name):
                removed.append(str(path.relative_to(output)))
                path.unlink()
        plist = app / 'Contents/Info.plist'
        info = plistlib.loads(plist.read_bytes())
        info.update(LSMinimumSystemVersion='10.15', CFBundlePackageType='APPL',
                    CFBundleShortVersionString=lock['version'], CFBundleVersion=lock['version'])
        plist.write_bytes(plistlib.dumps(info))
        for path in sorted(app.rglob('*')):
            if not path.is_file() or path.is_symlink():
                continue
            data = path.read_bytes()
            selected = select_slice(data, 'x86_64')
            if selected is None:
                continue
            if selected != data:
                mode = path.stat().st_mode
                temporary = path.with_name(path.name + '.intel-only')
                subprocess.run(['lipo', str(path), '-thin', 'x86_64', '-output', str(temporary)], check=True)
                temporary.chmod(mode)
                temporary.replace(path)
            _, dependencies = inspect_macho(path.read_bytes(), 'x86_64')
            for dependency in dependencies:
                if dependency.startswith('@rpath/Qt') and '.framework/Versions/5/' in dependency:
                    name = dependency.rsplit('/', 1)[1]
                    if not (binary_dir / name).is_file():
                        raise RuntimeError(f'Missing frozen Qt dependency {name}')
                    subprocess.run(['install_name_tool', '-change', dependency,
                                    '@executable_path/' + name, str(path)], check=True)
            subprocess.run(['codesign', '--force', '--sign', '-', str(path)], check=True)
        subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    native = apps[0] / 'Contents/MacOS/libcarla_native-plugin.dylib'
    runtime_native = args.normalized_runtime / 'lib/libcarla_native-plugin.dylib'
    if sections(native) != sections(runtime_native):
        raise RuntimeError('Standalone package native library code/data differs from ShoopDaLoop runtime')
    licenses = output / 'licenses'
    licenses.mkdir()
    shutil.copy2(args.license, licenses / 'Carla-GPL.txt')
    for path in (args.dependency_dir / 'licenses').iterdir():
        shutil.copy2(path, licenses / path.name)
    shutil.copy2(LOCK, output / 'runtime-lock.json')
    shutil.copy2(ROOT / 'docs/carla-catalina-ab.md', output / 'README.md')
    payloads = {str(p.relative_to(output)): p.read_bytes() for p in output.rglob('*') if p.is_file()}
    report = verify_payloads(payloads, 'x86_64', '10.15')
    (output / 'compatibility.txt').write_text('\n'.join(report) + '\n')
    manifest = {'purpose': 'Standalone Carla A/B test; no ShoopDaLoop executable involved',
                'version': lock['version'], 'architecture': 'x86_64', 'minimum_macos': '10.15',
                'upstream': lock['upstream_binaries']['macos-universal'],
                'source': lock['source'], 'dependencies': lock['macos_catalina_dependencies'],
                'recipe': 'Same pinned upstream Carla and rebuilt Catalina dependencies as bundled runtime; complete apps retained',
                'signing': 'Ad hoc only, not notarized', 'replaced': replacements, 'omitted_unused_qt_bindings': removed,
                'native_sections_match_bundled_runtime': True, 'native_sections': sections(native),
                'build_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'files': {str(p.relative_to(output)): digest(p) for p in output.rglob('*') if p.is_file()}}
    (output / 'BUILD-INFO.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'Staged {len(apps)} complete apps; audited {len(report)} Intel Mach-O files')


if __name__ == '__main__':
    main()
