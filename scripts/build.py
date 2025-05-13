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
default_vcpkgs_installed_path = os.path.join(base_path, "build", "vcpkg_installed")

def stream_reader(stream, mirror_stream):
    for c in iter(lambda: stream.read(1), b""):
        mirror_stream.write(c)

def run_and_print(command, env=None, err="Command failed.", cwd=None):
    print(f"-> Running command: {command}")
    result = subprocess.run(command, shell=True, env=env, cwd=cwd)
    if result.returncode != 0:
        print(f"-> Error: {err}")
        exit(1)

def find_qmake(directory):
    tail = os.path.join("Qt6", "bin", "qmake.exe") if sys.platform == "win32" else os.path.join("Qt6", "bin", "qmake")
    pattern = f'{directory}/**/{tail}'
    print(f"Looking for qmake at: {pattern}")
    qmake_paths = glob.glob(pattern, recursive=True)
    if not qmake_paths:
        return None
    qmake_path = qmake_paths[0]
    return qmake_path

def find_python(vcpkg_installed_directory, is_debug_build):
    tail = os.path.join("tools", "python3", ("python3.exe" if sys.platform == "win32" else "python3"))
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

    libname = f'python{major_version}.{minor_version}d' if is_debug_build else f'python{major_version}.{minor_version}'
    libdir = os.path.join(os.path.dirname(exe), '../../debug/lib' if is_debug_build else '../../lib')
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

def find_vcpkg_dynlibs_paths(installed_dir):
    # TODO: handle MacOS
    tail = os.path.join("bin", "zita-resampler.dll") if sys.platform == "win32" \
           else os.path.join("lib", "libzita-resampler.so") if sys.platform == "linux" \
           else os.path.join("lib", "libzita-resampler.dylib")
    pattern = f'{installed_dir}/**/{tail}'
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

def add_build_parser(subparsers):
    global default_vcpkgs_installed_path

    build_parser = subparsers.add_parser('build', help='Build the project')

    # default_python_version = os.environ.get('PYTHON_VERSION', '3.10')

    # build_parser.add_argument('--python-version', type=str, required=False, default=default_python_version, help='Python version to embed into ShoopDaLoop. Will be installed with uv if not already present.')
    build_parser.add_argument('--vcpkg-root', type=str, required=False, default=os.environ.get('VCPKG_ROOT'), help='Path to the VCPKG root directory. Default is VCPKG_ROOT environment variable.')
    build_mode_group = build_parser.add_mutually_exclusive_group()
    build_mode_group.add_argument('--debug', action='store_true', help='Build in debug mode.')
    build_mode_group.add_argument('--release', action='store_true', help='Build in release mode.')

    # build_parser.add_argument("--skip-python", action='store_true', help="Don't install Python and create a virtual environment (it should already be there from a previous build).")
    build_parser.add_argument("--skip-vcpkg", action='store_true', help="Don't install vcpkg packages (they should already be there from a previous build).")
    build_parser.add_argument('--skip-cargo', action='store_true', help="Don't build anything after the preparation steps.")
    build_parser.add_argument("--incremental", action='store_true', help="Implies --skip-python and --skip-vcpkg.")
    
    build_parser.add_argument("--vcpkg-installed-dir", type=str, default=default_vcpkgs_installed_path, help="Path where to install/find vcpkg packages.")
    build_parser.add_argument("--vcpkg-args", type=str, help="Additional arguments to pass to vcpkg install.", default=None)
    
    build_parser.add_argument('--cargo-args', '-c', type=str, help='Pass additional arguments to cargo build.', default='')

    build_parser.add_argument('--write-build-env-ps1', action='store_true', help="Write a sourcable script that sets the build env so cargo can be run manually. Implies --skip-cargo.")
    build_parser.add_argument('--write-build-env-sh', action='store_true', help="Write a bash script that sets the build env so cargo can be run manually. Implies --skip-cargo.")
    
def build(args):
    build_env = dict()

    def apply_build_env(env_dict):
        env = os.environ.copy()
        for key, value in env_dict.items():
            env[key] = value
        return env

    build_mode = ('release' if args.release else 'debug')
    print(f"Building in {build_mode} mode.")

    if args.incremental:
        args.skip_vcpkg = True
        args.skip_python = True

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
        result = subprocess.check_output(f'{vcpkg_exe} --help', shell=True, env=apply_build_env(build_env))
    except subprocess.CalledProcessError:
        print("Error: vcpkg not found in PATH. Please install it and ensure it is in the PATH.")

    # Setup VCPKG_ROOT and toolchain file and triplets
    # Note that we use our own triplets that ensure dynamic linkage of libraries and MacOS version choosing.
    if not args.vcpkg_root:
        print(f"Error: VCPKG_ROOT environment variable is not set, nor passed using --vcpkg-root. Please install vcpkg and pass its root accordingly.")
        exit(1)
    build_env['VCPKG_ROOT'] = args.vcpkg_root
    build_env['VCPKG_OVERLAY_TRIPLETS'] = os.path.join(base_path, "vcpkg", "triplets")
    build_env['VCPKG_OVERLAY_PORTS'] = os.path.join(base_path, "vcpkg", "ports")
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

    # Setup cargo
    try:
        result = subprocess.check_output('cargo -V', shell=True, env=apply_build_env(build_env))
    except subprocess.CalledProcessError:
        print("Error: cargo not found in PATH. Please install it and ensure it is in the PATH.")


    print("Tool and environment checks done.")

    # Install vcpkg packages first
    vcpkg_installed_dir = args.vcpkg_installed_dir
    build_env["VCPKG_INSTALLED_DIR"] = vcpkg_installed_dir
    if args.skip_vcpkg:
        print(f"Skipping vcpkg setup: assuming packages are already in {vcpkg_installed_dir}.")
    else:
        print("Installing vcpkg packages...")
        extra_args = args.vcpkg_args if args.vcpkg_args else ''
        run_and_print(f"{vcpkg_exe} install --x-install-root={vcpkg_installed_dir} {extra_args}",
                        env=apply_build_env(build_env),
                        cwd=os.path.join(base_path, 'vcpkg'),
                        err="Failed to fetch/build/install vcpkg packages.")
        print("vcpkg packages installed.")
    vcpkg_installed_prefix = os.path.join(vcpkg_installed_dir, detect_vcpkg_triplet())
    build_env["CMAKE_PREFIX_PATH"] = vcpkg_installed_prefix

    # Find python
    (python_exe, python_libdir, python_libname, python_version) = find_python(vcpkg_installed_dir, build_mode=='debug')
    print(f"Found python at: {python_exe}")
    pyo3_config_file=os.path.join(base_path, 'build', 'pyo3_config.toml')
    with open(pyo3_config_file, 'w') as f:
        f.write(f"shared=true\n")
        f.write(f"lib_name={python_libname}\n")
        f.write(f"lib_dir={python_libdir}\n")
        f.write(f"executable={python_exe}\n")
        f.write(f"version={python_version}\n")
    build_env["PYO3_CONFIG_FILE"] = pyo3_config_file

    # Find qmake
    qmake_path = find_qmake(vcpkg_installed_dir)
    if not qmake_path:
        print("Error: qmake not found in vcpkg packages.")
        sys.exit(1)
    print(f"Found qmake at: {qmake_path}")
    build_env["QMAKE"] = qmake_path

    if args.write_build_env_ps1:
        args.skip_cargo = True
        env_filename = f".build-env-{build_mode}.ps1"
        env_file = os.path.join(base_path, env_filename)
        print("Writing the build env to a .ps1 file.")
        with open(env_file, "w") as f:
            for key, value in build_env.items():
                f.write(f'$env:{key}="{value}"\n')
        print(f'\nWrote the build env to {env_file}. Apply it using:')
        print(f'\n    . ./{env_filename}')
        print('\nThen build using cargo, e.g.:')
        print('\n    cargo build')

    if args.write_build_env_sh:
        args.skip_cargo = True
        env_filename = f".build-env-{build_mode}.sh"
        env_file = os.path.join(base_path, env_filename)
        print("Writing the build env to a .sh file.")
        with open(env_file, "w") as f:
            for key, value in build_env.items():
                f.write(f"""export {key}="{windows_to_bash_paths(value) if sys.platform == 'win32' else value}"\n""")
        print(f'\nWrote the build env to {env_file}. Apply it using:')
        print(f'\n    . ./{env_filename}')
        print('\nThen build using cargo, e.g.:')
        print('\n    cargo build')

    if not args.skip_cargo:
        # Run the build
        print("Preparations and checks done. Starting the cargo build.")
        run_and_print(f"cargo build {('--release' if build_mode == 'release' else '')} {args.cargo_args}",
                        env=apply_build_env(build_env),
                        err="Failed to build the project.")
        print("\nBuild finished.")

        dev_exe = os.path.join(".", "target", build_mode, "shoopdaloop_dev")
        dev_launcher = os.path.join(".", "target", build_mode, "shoopdaloop_windows_launcher.exe")
        run_dev = (dev_launcher if sys.platform == "win32" else dev_exe)

        print("You can now run the project in dev mode by running:")
        print(f"\n   {run_dev}")
        if sys.platform == 'win32':
            print("\nWith the following in your PATH:")
            print(f"\n   {dynlib_path}")
        print("\nTo explore packaging options, run:")
        print(f"\n   ./target/{build_mode}/package --help\n")
    
def main():
    parser = argparse.ArgumentParser(description='ShoopDaLoop build script')
    subparsers = parser.add_subparsers(dest='command')

    # Add sub-parsers
    add_build_parser(subparsers)

    (args, remainder) = parser.parse_known_args(sys.argv[1:])

    if args.command == 'build':
        # Strict parsing
        args = parser.parse_args(sys.argv[1:])
        build(args)

if __name__ == '__main__':
    main()