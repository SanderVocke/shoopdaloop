import argparse
import os
import subprocess
import sys
import threading
import glob
import urllib.request
import shutil
import zipfile

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

import platform
import sys

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

def find_vcpkg_dynlibs_paths(installed_dir):
    # TODO: handle MacOS
    tail = os.path.join("bin", "zita-resampler.dll") if sys.platform == "win32" \
           else os.path.join("lib", "libzita-resampler.so")
    pattern = f'{installed_dir}/**/{tail}'
    print(f"Looking for dynamic libraries by searching for zita-resampler at: {pattern}")
    zita_paths = glob.glob(pattern, recursive=True)
    if not zita_paths:
        return None
    zita_path = zita_paths[0]
    dynlib_path = os.path.dirname(zita_path)
    print(f"Found dynamic library path at: {dynlib_path}")
    return dynlib_path

def add_build_parser(subparsers):
    global default_vcpkgs_installed_path

    build_parser = subparsers.add_parser('build', help='Build the project')

    default_python_version = os.environ.get('PYTHON_VERSION', '3.10')

    build_parser.add_argument('--python-version', type=str, required=False, default=default_python_version, help='Python version to embed into ShoopDaLoop. Will be installed with uv if not already present.')
    build_parser.add_argument('--vcpkg-root', type=str, required=False, default=os.environ.get('VCPKG_ROOT'), help='Path to the VCPKG root directory. Default is VCPKG_ROOT environment variable.')
    build_mode_group = build_parser.add_mutually_exclusive_group()
    build_mode_group.add_argument('--debug', action='store_true', help='Build in debug mode.')
    build_mode_group.add_argument('--release', action='store_true', help='Build in release mode.')

    build_parser.add_argument("--skip-python", action='store_true', help="Don't install Python and create a virtual environment (it should already be there from a previous build).")
    build_parser.add_argument("--skip-vcpkg", action='store_true', help="Don't install vcpkg packages (they should already be there from a previous build).")
    build_parser.add_argument('--skip-cargo', action='store_true', help="Don't build anything after the preparation steps.")
    build_parser.add_argument("--incremental", action='store_true', help="Implies --skip-python and --skip-vcpkg.")
    build_parser.add_argument("--vcpkg-installed-dir", type=str, default=default_vcpkgs_installed_path, help="Path where to install/find vcpkg packages. WARNING: if used, this should be passed identically to the 'package' command.")
    
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
    vcpkg_exe = os.path.join(args.vcpkg_root, "vcpkg")
    if sys.platform == 'win32':
        vcpkg_exe += ".exe"

    # Setup vcpkg
    try:
        result = subprocess.check_output(f'{vcpkg_exe} --help', shell=True, env=apply_build_env(build_env))
    except subprocess.CalledProcessError:
        print("Error: vcpkg not found in PATH. Please install it and ensure it is in the PATH.")

    # Setup VCPKG_ROOT and toolchain file
    if not args.vcpkg_root:
        print(f"Error: VCPKG_ROOT environment variable is not set, nor passed using --vcpkg-root. Please install vcpkg and pass its root accordingly.")
        exit(1)
    build_env['VCPKG_ROOT'] = args.vcpkg_root
    vcpkg_toolchain = os.path.join(build_env['VCPKG_ROOT'], "scripts", "buildsystems", "vcpkg.cmake")
    if sys.platform == 'macOS':
        vcpkg_toolchain_wrapper = os.path.join(base_path, "build", "vcpkg.cmake")
        # TODO: for some reason, in particular for MacOS on ARM, we need to
        # pass the target triplet. Env vars or cache entries don't work, so make a toolchain file wrapper
        os.makedirs(os.path.dirname(vcpkg_toolchain_wrapper), exist_ok=True)
        with open(vcpkg_toolchain_wrapper, "w") as f:
            f.write(f"""set(VCPKG_TARGET_TRIPLET "{detect_vcpkg_triplet()}")\n""")
            f.write(f"""include("{vcpkg_toolchain}")\n""")
        vcpkg_toolchain = vcpkg_toolchain_wrapper
    build_env["CMAKE_TOOLCHAIN_FILE"] = vcpkg_toolchain
    print(f"Using VCPKG_ROOT: {build_env['VCPKG_ROOT']}")

    # Setup Python version
    python_version = args.python_version
    print(f"Using Python version: {python_version}")
    if args.skip_python:
        print(f"Skipping Python installation: assuming Python {python_version} is already installed.")
    else:
        run_and_print(f"uv python install {python_version}",
                        env=apply_build_env(build_env),
                        err="Couldn't find find/install Python version.")
        print(f"-> Python {python_version} found.")

    # Setup venv
    venv_path = os.path.join(base_path, "build", "venv")
    python_command = os.path.join(venv_path, "bin", "python") if sys.platform != "win32" else os.path.join(venv_path, "Scripts", "python.exe")

    if args.skip_python:
        print(f"Skipping venv setup: assuming build/venv is already installed.")
    else:
        print(f"Setting up build venv")
        print(f"-> Venv path: {venv_path}")
        run_and_print(f"uv venv --python {python_version} {venv_path}",
                        env=apply_build_env(build_env),
                        err="Couldn't create/check venv.")
        run_and_print(f"uv pip install --python {python_command} -r {base_path}/build_python_requirements.txt",
                        env=apply_build_env(build_env),
                        err="Couldn't find/install python dependencies.")
    python_base_prefix = subprocess.check_output([python_command, '-c', 'import sys; print(sys.base_prefix)'], stderr=subprocess.DEVNULL).decode().strip()
    python_base_interpreter = os.path.join(python_base_prefix, "bin", "python") if sys.platform != "win32" else os.path.join(python_base_prefix, "python.exe")
    build_env["PYTHON"] = python_command
    build_env["PYO3_PYTHON"] = python_base_interpreter

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
        run_and_print(f"{vcpkg_exe} install --x-install-root={vcpkg_installed_dir}",
                        env=apply_build_env(build_env),
                        cwd=os.path.join(base_path, 'src', 'backend'),
                        err="Failed to fetch/build/install vcpkg packages.")
        print("vcpkg packages installed.")

    # Find qmake
    qmake_path = find_qmake(vcpkg_installed_dir)
    if not qmake_path:
        print("Error: qmake not found in vcpkg packages.")
        sys.exit(1)
    print(f"Found qmake at: {qmake_path}")
    build_env["QMAKE"] = qmake_path

    # Find dynamic library folders and add to path
    dynlib_path = find_vcpkg_dynlibs_paths(args.vcpkg_installed_dir)
    if sys.platform == 'win32':
        build_env['PATH'] = f"{(build_env['PATH'] + os.pathsep) if 'PATH' in build_env else (os.environ['PATH'] + os.pathsep)}{dynlib_path}"
    elif sys.plagform == 'linux':
        build_env['LD_LIBRARY_PATH'] = f"{(build_env['LD_LIBRARY_PATH'] + os.pathsep) if 'LD_LIBRARY_PATH' in build_env else (os.environ['LD_LIBRARY_PATH'] + os.pathsep)}{dynlib_path}"
        build_env['SHOOPDALOOP_DEV_EXTRA_DYLIB_PATH'] = dynlib_path
    elif sys.platform == 'darwin':
        build_env['DYLD_LIBRARY_PATH'] = f"{(build_env['DYLD_LIBRARY_PATH'] + os.pathsep) if 'DYLD_LIBRARY_PATH' in build_env else (os.environ['DYLD_LIBRARY_PATH'] + os.pathsep)}{dynlib_path}"
        build_env['SHOOPDALOOP_DEV_EXTRA_DYLIB_PATH'] = dynlib_path      

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
                f.write(f'export {key}="{value}"\n')
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
        print(f"\n   [build.ps1|build.sh|build.py] package --help\n")

def add_package_parser(subparsers):
    global default_vcpkgs_installed_path

    package_parser = subparsers.add_parser('package', add_help=False)
    package_parser.add_argument('--help', '-h', action='store_true')

    package_parser.add_argument("--vcpkg-installed-dir", type=str, default=default_vcpkgs_installed_path, help="Path where to install/find vcpkg packages built in the build stage.")

    build_mode_group = package_parser.add_mutually_exclusive_group()
    build_mode_group.add_argument('--debug', action='store_true')
    build_mode_group.add_argument('--release', action='store_true')

def package(args, remainder):
    package_env = os.environ.copy()
    build_mode = ('release' if args.release else 'debug')
    package_exe = os.path.join(".", "target", build_mode, ("package.exe" if sys.platform == 'win32' else "package"))
    print_help = args.help and len(remainder) == 0

    if print_help:
        print('Usage: build.py package (--debug|--release) [options]')
        print('')
        print('This script is a wrapper around the target/(debug|release)/package(.exe) tool built in the build step.')
        print('Arguments are passed to package.exe, after doing some env setup and checks first.')
        print('Passing --debug or --release chooses the package.exe from the debug or release build.')
        print('')

    if not os.path.exists(package_exe):
        print(f'Error: {package_exe} does not exist. Please run a build first.')
        sys.exit(1)
    
    if print_help:
        print(f"Following is the output of {package_exe} --help. You can directly pass these options to this script.")
        print('')
        subprocess.run([package_exe, "--help"])
        sys.exit(0)

    # Check for Dependencies.exe, which is needed for basically any packaging step on windows.
    if sys.platform == 'win32' and not shutil.which('Dependencies.exe'):
        dependencies_folder = os.path.join(base_path, 'build', 'Dependencies.exe')
        dependencies_executable = os.path.join(dependencies_folder, 'Dependencies.exe')
        if not os.path.exists(dependencies_executable):
            print('Did not find Dependencies.exe - downloading.')
            os.makedirs(dependencies_folder)
            zip_file = os.path.join(dependencies_folder, 'dependencies.zip')
            urllib.request.urlretrieve(
                'https://github.com/lucasg/Dependencies/releases/download/v1.11.1/Dependencies_x64_Release.zip',
                zip_file)
            with zipfile.ZipFile(zip_file, 'r') as zip_ref:
                zip_ref.extractall(dependencies_folder)
        package_env['PATH'] = f"{package_env['PATH']};{dependencies_folder}"
    
    # Find QMake
    qmake_path = find_qmake(args.vcpkg_installed_dir)
    if not qmake_path:
        print("Error: qmake not found in vcpkg packages.")
        sys.exit(1)
    print(f"Found qmake at: {qmake_path}")
    package_env["QMAKE"] = qmake_path

    # Find dynamic library folders and add to path
    dynlib_path = find_vcpkg_dynlibs_paths(args.vcpkg_installed_dir)
    if sys.platform == 'win32':
        build_env['PATH'] = f"{package_env['PATH']}{os.pathsep}{dynlib_path}"
    elif sys.plagform == 'linux':
        package_env['LD_LIBRARY_PATH'] = dynlib_path
    elif sys.platform == 'darwin':
        package_env['DYLD_LIBRARY_PATH'] = dynlib_path   

    tool_args = [a for a in sys.argv[1:] if a not in ['package', '--debug', '--release']]
    cmd = [package_exe, *tool_args]
    print(f'Running: {" ".join(cmd)}')
    print('')
    print(f'{package_exe} output:')
    print('')
    result = subprocess.run(cmd, env=package_env)
    sys.exit(result.returncode)
    
def main():
    parser = argparse.ArgumentParser(description='ShoopDaLoop build script')
    subparsers = parser.add_subparsers(dest='command')

    # Add sub-parsers
    add_build_parser(subparsers)
    add_package_parser(subparsers)

    (args, remainder) = parser.parse_known_args(sys.argv[1:])

    if args.command == 'build':
        # Strict parsing
        args = parser.parse_args(sys.argv[1:])
        build(args)
    elif args.command == 'package':
        package(args, remainder)

if __name__ == '__main__':
    main()