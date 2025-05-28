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
    env_settings = dict()

    win_qmake = 'qmake.debug.bat' if is_debug_build else 'qmake.exe'
    tail = os.path.join("Qt6", "bin", win_qmake) if sys.platform == "win32" else os.path.join("Qt6", "bin", "qmake")
    pattern = f'{directory}/**/{tail}'
    print(f"Looking for qmake at: {pattern}")
    qmake_paths = glob.glob(pattern, recursive=True)
    if not qmake_paths:
        return (None, None)
    qmake = qmake_paths[0]

    return (qmake, env_settings)

def find_python(vcpkg_installed_directory, is_debug_build):
    executable_release = ("python.exe" if sys.platform == "win32" else "python3")
    executable_debug = ("python_d.exe" if sys.platform == "win32" else "python3d")
    executable = executable_debug if is_debug_build else executable_release
    tail = os.path.join("tools", "python3", executable)
    if is_debug_build:
        tail = os.path.join("debug", tail)
    pattern = f'{vcpkg_installed_directory}/**/{tail}'
    python_paths = glob.glob(pattern, recursive=True)
    exe = (python_paths[0] if python_paths else None)

    if not exe or not os.path.isfile(exe):
        print(f"Couldn't find vcpkg-built python interpreter @ {pattern}")
        exit(1)

    # query the python version number by running the executable
    try:
        result = subprocess.run([exe, "--version"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        version_output = result.stdout.decode('utf-8')
        # Extract the major and minor version numbers
        match = re.search(r'Python (\d+)\.(\d+)', version_output)
        if not match:
            raise ValueError("Couldn't extract Python version from output")
        major_version, minor_version = map(int, match.groups())
        print(f"-> Found python version: {major_version}.{minor_version}")
    except Exception as e:
        print(f"-> Error: {e}")
        exit(1)

    dbg_suffix = '_d' if sys.platform == 'win32' else 'd'
    maybe_dot = '' if sys.platform == 'win32' else '.'
    libname = f'python{major_version}{maybe_dot}{minor_version}{dbg_suffix}' if is_debug_build else f'python{major_version}{maybe_dot}{minor_version}'
    libdir = os.path.join(os.path.dirname(exe), '../../../debug/lib' if is_debug_build else '../../lib')
    version = f'{major_version}.{minor_version}'

    return (exe, libdir, libname, version)

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

def find_vcpkg_dynlibs_paths(installed_dir, is_debug_build):
    # TODO: handle MacOS
    tail = os.path.join("bin", "zita-resampler.dll") if sys.platform == "win32" \
           else os.path.join("lib", "libzita-resampler.so") if sys.platform == "linux" \
           else os.path.join("lib", "libzita-resampler.dylib")
    dbgpart = "debug/" if is_debug_build else ""
    pattern = f'{installed_dir}/{dbgpart}**/{tail}'
    print(f"Looking for dynamic libraries by searching for zita-resampler at: {pattern}")
    zita_paths = glob.glob(pattern, recursive=True)
    if not zita_paths:
        return None
    zita_path = zita_paths[0]
    dynlib_path = os.path.dirname(zita_path)
    if sys.platform == "win32":
        dynlib_path = dynlib_path.replace('/', '\\')
    print(f"Found dynamic library path at: {dynlib_path}")
    return dynlib_path

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
    vcpkg_exe = os.path.join(args.vcpkg_root, "vcpkg")
    if sys.platform == 'win32':
        vcpkg_exe += ".exe"

    # Setup vcpkg
    try:
        result = subprocess.check_output(f'{vcpkg_exe} --help', shell=True, env=apply_build_env(new_build_env))
    except subprocess.CalledProcessError:
        print("Error: vcpkg not found in PATH. Please install it and ensure it is in the PATH.")
        exit(1)

    # Setup VCPKG_ROOT and toolchain file and triplets
    # Note that we use our own triplets that ensure dynamic linkage of libraries and MacOS version choosing.
    if not args.vcpkg_root:
        print(f"Error: VCPKG_ROOT environment variable is not set, nor passed using --vcpkg-root. Please install vcpkg and pass its root accordingly.")
        exit(1)
    new_build_env['VCPKG_ROOT'] = args.vcpkg_root
    new_build_env['VCPKG_OVERLAY_TRIPLETS'] = os.path.join(base_path, "vcpkg", "triplets")
    new_build_env['VCPKG_OVERLAY_PORTS'] = os.path.join(base_path, "vcpkg", "ports")
    vcpkg_toolchain = os.path.join(new_build_env['VCPKG_ROOT'], "scripts", "buildsystems", "vcpkg.cmake")
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
    print(f"Using VCPKG_ROOT: {new_build_env['VCPKG_ROOT']}")

    # Install vcpkg packages first
    vcpkg_installed_dir = args.vcpkg_installed_dir
    new_build_env["VCPKG_INSTALLED_DIR"] = vcpkg_installed_dir
    if args.skip_vcpkg:
        print(f"Skipping vcpkg setup: assuming packages are already in {vcpkg_installed_dir}.")
    else:
        print("Installing vcpkg packages...")
        extra_args = args.vcpkg_args if args.vcpkg_args else ''
        run_and_print(f"{vcpkg_exe} install --x-install-root={vcpkg_installed_dir} {extra_args}",
                        env=apply_build_env(new_build_env),
                        cwd=os.path.join(base_path, 'vcpkg'),
                        err="Failed to fetch/build/install vcpkg packages.")
        print("vcpkg packages installed.")
    vcpkg_installed_prefix = os.path.join(vcpkg_installed_dir, detect_vcpkg_triplet())
    new_build_env["CMAKE_PREFIX_PATH"] = vcpkg_installed_prefix
    if sys.platform == 'win32':
        pkgconf_dir = os.path.dirname(find_vcpkg_pkgconf(vcpkg_installed_dir))
        new_build_env = add_to_env_paths('PATH', pkgconf_dir, new_build_env)

    return new_build_env

def generate_env(args, env, is_debug):
    build_env = env.copy()

    # Find python
    (python_exe, python_libdir, python_libname, python_version) = find_python(args.vcpkg_installed_dir, is_debug)
    print(f"Found python at: {python_exe}")
    pyo3_config_file=os.path.join(base_path, 'build', f'pyo3-config-{("debug" if is_debug else "release")}.toml')
    with open(pyo3_config_file, 'w') as f:
        f.write(f"shared=true\n")
        f.write(f"lib_name={python_libname}\n")
        f.write(f"lib_dir={python_libdir}\n")
        f.write(f"executable={python_exe}\n")
        f.write(f"version={python_version}\n")
    build_env["PYO3_CONFIG_FILE"] = pyo3_config_file
    build_env["SHOOP_DEV_ENV_PYTHON"] = python_exe

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
    
    if not args.skip_vcpkg:
        general_env = build_vcpkg(args, general_env)
    
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

    print(f'\nWrote build environment files to build/build-env-[debug|release].[sh|ps1].')
    print('Apply the debug or release environment file by sourcing the relevant script.')
    print('\nThen build using cargo, e.g.:')
    print('\n    cargo build [--release]')

if __name__ == '__main__':
    main()