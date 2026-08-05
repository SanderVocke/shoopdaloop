import argparse
import os
import subprocess
import sys
import threading
import glob
import urllib.request
import shutil
import zipfile
import re

script_path = os.path.dirname(os.path.realpath(__file__))
base_path = script_path + '/..'

def stream_reader(stream, mirror_stream):
    for c in iter(lambda: stream.read(1), b""):
        mirror_stream.write(c)

def run_and_print(command, env=None, err="Command failed.", cwd=None):
    print(f"-> Running command: {command}")
    result = subprocess.run(command, shell=True, env=env, cwd=cwd)
    if result.returncode != 0:
        print(f"-> Error: {err}")
        exit(1)

def find_qmake(directory, is_debug_build):
    """Locate the qmake that cxx-qt and the packaging step should use.

    Always the *release* qmake, including for debug builds.

    rustc on MSVC links the release C runtime and has no `/MDd` equivalent, so a
    debug-built Qt can never match the Rust side of this application. Pointing
    cxx-qt at vcpkg's debug Qt produced a package holding both the debug and the
    release CRT, which do not share a heap -- the debug Windows portable folder
    crashed on startup as a result. Linking release Qt in a debug build costs Qt's
    own asserts and symbols and buys a package that runs.

    Non-Windows platforms already behaved this way: the debug branch this replaces
    was guarded on win32, so plain `qmake` was picked up unconditionally there.

    The trade-off is deliberate: Windows debug CI must use release Qt so all
    Rust and C++ components agree on the release CRT.
    """
    del is_debug_build  # release qmake regardless -- see above

    env_settings = dict()
    win_qmake = 'qmake.exe' if sys.platform == "win32" else 'qmake'
    tail = os.path.join("Qt6", "bin", win_qmake)
    pattern = f'{directory}/**/{tail}'
    print(f"Looking for qmake at: {pattern}")
    qmake_paths = glob.glob(pattern, recursive=True)
    if not qmake_paths:
        return (None, None)

    return (qmake_paths[0], env_settings)

def windows_to_bash_path(windows_path):
    # Match drive letter at the beginning of the path (e.g., C:\ or D:/)
    match = re.match(r'^([A-Za-z]):[\\/](.*)', windows_path)
    if match:
        drive = match.group(1).lower()
        rest = match.group(2).replace('\\', '/')
        return f'/{drive}/{rest}'
    else:
        # If no drive letter prefix, return with backslashes converted
        return windows_path.replace('\\', '/')
    
def detect_vcpkg_triplet():
    system = platform.system()
    machine = platform.machine().lower()
    # Normalize architecture names
    arch = {
        'amd64': 'x64',
        'x86_64': 'x64',
        'arm64': 'arm64',
        'aarch64': 'arm64',
    }.get(machine, machine)
    # Match OS and arch to triplets
    if system == 'Windows':
        if arch in ('x64', 'arm64'):
            return f'{arch}-windows'
    elif system == 'Linux':
        if arch in ('x64', 'arm64'):
            return f'{arch}-linux'
    elif system == 'Darwin':
        if arch in ('x64', 'arm64'):
            return f'{arch}-osx'
    return 'unknown-unknown'
    
def windows_to_bash_paths(windows_paths):
    return ':'.join(windows_to_bash_path(path) for path in windows_paths.split(';')) if windows_paths else windows_paths

import platform
import sys

def find_vcpkg_pkgconf(installed_dir):
    filename = 'pkgconf'
    if sys.platform == 'win32':
        filename = filename + ".exe"
    tail = os.path.join('tools', 'pkgconf', filename)
    pattern = f'{installed_dir}/**/{tail}'
    print(f"Looking for {filename} by searching for at: {pattern}")
    paths = glob.glob(pattern, recursive=True)
    if not paths:
        return None
    path = paths[0]
    print(f'found pkgconf at: {path}')
    return path

def apply_build_env(env_dict):
    env = os.environ.copy()
    for key, value in env_dict.items():
        env[key] = value
    return env
    
def add_to_env_paths(varname, path, env):
    new_env = env.copy()
    sep = ';' if sys.platform == 'win32' else ':'
    if not varname in new_env:
        new_env[varname] = os.environ.get(varname) or ''
    new_env[varname] = f'{new_env[varname]}{sep}{path}'
    return new_env
    
def add_vcpkg_env(args, env):
    new_env = env.copy()
    new_env['VCPKG_OVERLAY_TRIPLETS'] = os.path.join(base_path, "vcpkg", "triplets")
    new_env['VCPKG_OVERLAY_PORTS'] = os.path.join(base_path, "vcpkg", "ports")
    new_env["VCPKG_INSTALLED_DIR"] = args.vcpkg_installed_dir
    triplet_dir = os.path.join(args.vcpkg_installed_dir, detect_vcpkg_triplet())
    new_env["CMAKE_PREFIX_PATH"] = triplet_dir
    # Make vcpkg-installed pkg-config files (e.g. lilv-0.pc) discoverable so
    # crates like `lilv` can resolve their system dependencies via pkg-config.
    new_env["PKG_CONFIG_PATH"] = os.pathsep.join([
        os.path.join(triplet_dir, "lib", "pkgconfig"),
        os.path.join(triplet_dir, "share", "pkgconfig"),
        os.path.join(triplet_dir, "debug", "lib", "pkgconfig"),
    ])
    return new_env

def build_vcpkg(args, build_env):
    new_build_env = build_env.copy()

    maybe_vcpkg_root = os.environ.get('VCPKG_ROOT')
    if maybe_vcpkg_root and not args.vcpkg_root:
        print(f"Using VCPKG_ROOT from env: {maybe_vcpkg_root}")
        args.vcpkg_root = maybe_vcpkg_root
    elif not args.vcpkg_root:
        if os.path.exists(os.path.join(base_path, "build", "vcpkg")):
            print(f"Using VCPKG_ROOT from build dir: {os.path.join(base_path, 'build', 'vcpkg')}")
            args.vcpkg_root = os.path.join(base_path, "build", "vcpkg")
        else:
            print(f"vcpkg not found. Bootstrapping...")
            # clone the vcpkg repo into build/vcpkg
            os.makedirs(os.path.join(base_path, "build"), exist_ok=True)
            subprocess.run(f'git clone https://github.com/microsoft/vcpkg.git {os.path.join(base_path, "build", "vcpkg")}', shell=True)
            # bootstrap vcpkg
            if sys.platform == 'win32':
                subprocess.run('bootstrap-vcpkg.bat', cwd=os.path.join(base_path, "build", "vcpkg"), shell=True)
            else:
                subprocess.run('./bootstrap-vcpkg.sh', cwd=os.path.join(base_path, "build", "vcpkg"), shell=True)
            args.vcpkg_root = os.path.join(base_path, "build", "vcpkg")
    else:
        print(f'VCPKG_ROOT provided from env: {args.vcpkg_root}')
    build_env['VCPKG_ROOT'] = args.vcpkg_root

    vcpkg_exe = os.path.join(args.vcpkg_root, "vcpkg")
    if sys.platform == 'win32':
        vcpkg_exe += ".exe"

    # Setup vcpkg
    print(build_env)
    try:
        result = subprocess.check_output(f'{vcpkg_exe} --help', shell=True, env=apply_build_env(build_env))
    except subprocess.CalledProcessError:
        print("Error: vcpkg not found.")
        exit(1)

    # Setup VCPKG_ROOT and toolchain file and triplets
    # Note that we use our own triplets that ensure dynamic linkage of libraries and MacOS version choosing.
    if not args.vcpkg_root:
        print(f"Error: VCPKG_ROOT environment variable is not set, nor passed using --vcpkg-root. Please install vcpkg and pass its root accordingly.")
        exit(1)
    vcpkg_toolchain = os.path.join(build_env['VCPKG_ROOT'], "scripts", "buildsystems", "vcpkg.cmake")
    if sys.platform == 'darwin':
        vcpkg_triplet = detect_vcpkg_triplet()
        vcpkg_toolchain_wrapper = os.path.join(base_path, "build", "vcpkg-toolchain.cmake")
        # TODO: for some reason, in particular for MacOS on ARM, we need to
        # pass the target triplet - even if we don't use a custom one.
        # Env vars or cache entries don't work, so make a toolchain file wrapper
        os.makedirs(os.path.dirname(vcpkg_toolchain_wrapper), exist_ok=True)
        with open(vcpkg_toolchain_wrapper, "w") as f:
            f.write(f"""set(VCPKG_TARGET_TRIPLET "{vcpkg_triplet}")\n""")
            f.write(f"""include("{vcpkg_toolchain}")\n""")
        with open(vcpkg_toolchain_wrapper, 'r') as f:
            print(f"Using toolchain file wrapper with contents:\n--------\n{f.read()}\n--------")
        vcpkg_toolchain = vcpkg_toolchain_wrapper
    print(f"Using VCPKG_ROOT: {build_env['VCPKG_ROOT']}")

    # Install vcpkg packages first
    if args.skip_vcpkg:
        print(f"Skipping vcpkg setup: assuming packages are already in {args.vcpkg_installed_dir}.")
    else:
        print("Installing vcpkg packages...")
        extra_args = args.vcpkg_args if args.vcpkg_args else ''
        run_and_print(f"{vcpkg_exe} install --x-install-root={args.vcpkg_installed_dir} {extra_args}",
                        env=apply_build_env(build_env),
                        cwd=os.path.join(base_path, 'vcpkg'),
                        err="Failed to fetch/build/install vcpkg packages.")
        print("vcpkg packages installed.")

    return build_env

def generate_env(args, env, is_debug):
    build_env = env.copy()

    # Find qmake
    (qmake_path, qmake_env) = find_qmake(args.vcpkg_installed_dir, is_debug)
    if not qmake_path:
        print("Error: qmake not found in vcpkg packages.")
        sys.exit(1)
    print(f"Found qmake at: {qmake_path}")
    build_env["QMAKE"] = qmake_path
    for key, value in qmake_env.items():
        print(f"using extra qmake env: {qmake_env}")
        build_env[key] = value

    # Find Lua
    build_env["LUA_LIB_NAME"] = "lua"
    
    return build_env
    
def main():
    default_vcpkgs_installed_path = os.path.join(base_path, "build", "vcpkg_installed")
    parser = argparse.ArgumentParser(description='ShoopDaLoop vcpkg prebuild script')
    parser.add_argument('--vcpkg-root', type=str, required=False, default=os.environ.get('VCPKG_ROOT'), help='Path to the VCPKG root directory. Default is VCPKG_ROOT environment variable.')
    parser.add_argument("--skip-vcpkg", action='store_true', help="Don't install vcpkg packages (they should already be there from a previous build).")
    parser.add_argument("--vcpkg-installed-dir", type=str, default=default_vcpkgs_installed_path, help="Path where to install/find vcpkg packages.")
    parser.add_argument("--vcpkg-args", type=str, help="Additional arguments to pass to vcpkg install.", default=None)
    args = parser.parse_args(sys.argv[1:])

    general_env = dict()
    general_env = add_vcpkg_env(args, general_env)
    
    if not args.skip_vcpkg:
        general_env = build_vcpkg(args, general_env)
    
    if sys.platform == 'win32':
        pkgconf_dir = os.path.dirname(find_vcpkg_pkgconf(args.vcpkg_installed_dir))
        general_env = add_to_env_paths('PATH', pkgconf_dir, general_env)
    
    debug_env = generate_env(args, general_env, True)
    release_env = generate_env(args, general_env, False)

    for variant in [
        ("debug", debug_env),
        ("release", release_env)
    ]:
        env_filename = f"build-env-{variant[0]}.ps1"
        env_file = os.path.join(base_path, "build", env_filename)
        print(f"Writing {env_filename}.")
        with open(env_file, "w") as f:
            for key, value in variant[1].items():
                f.write(f'$env:{key}="{value}"\n')

    for variant in [
        ("debug", debug_env),
        ("release", release_env)
    ]:
        env_filename = f"build-env-{variant[0]}.sh"
        env_file = os.path.join(base_path, "build", env_filename)
        print(f"Writing {env_filename}.")
        with open(env_file, "w") as f:
            for key, value in variant[1].items():
                f.write(f"""export {key}="{windows_to_bash_paths(value) if sys.platform == 'win32' else value}"\n""")
    
    for variant in [
        ("debug", debug_env),
        ("release", release_env)
    ]:
        env_filename = f"build-env-{variant[0]}.elv"
        env_file = os.path.join(base_path, "build", env_filename)
        print(f"Writing {env_filename}.")
        with open(env_file, "w") as f:
            for key, value in variant[1].items():
                value = value.replace('\\', '\\\\')
                f.write(f'set E:{key} = "{value}"\n')

    print(f'\nWrote build environment files to build/build-env-[debug|release].[sh|ps1].')
    print('Apply the debug or release environment file by sourcing the relevant script.')
    print('\nThen build using cargo, e.g.:')
    print('\n    cargo build [--release / --profile release-with-debug]')

if __name__ == '__main__':
    main()