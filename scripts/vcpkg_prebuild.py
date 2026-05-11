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

# cxx-qt needs a qmake binary, and does not accept a .bat file on Windows.
# however, a bat file is what is offered for debug on vcpkg builds.
# we compile a wrapper to act as a proxy to the bat file.
QMAKE_DEBUG_WRAPPER_C = r"""
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char* argv[]) {
    char self_path[MAX_PATH];
    DWORD len = GetModuleFileNameA(NULL, self_path, MAX_PATH);
    if (len == 0 || len == MAX_PATH) {
        fprintf(stderr, "qmake-debug-wrapper: GetModuleFileName failed\n");
        return 1;
    }

    char* last_sep = strrchr(self_path, '\\');
    if (!last_sep) last_sep = strrchr(self_path, '/');
    if (!last_sep) {
        fprintf(stderr, "qmake-debug-wrapper: cannot determine own directory\n");
        return 1;
    }
    *(last_sep + 1) = '\0';

    char bat_path[MAX_PATH];
    snprintf(bat_path, MAX_PATH, "%sqmake.debug.bat", self_path);

    /* Build command line: cmd.exe /c ""bat_path" arg1 arg2 ..." */
    /* The extra outer quotes are needed for cmd.exe /c to handle spaces correctly. */
    size_t cmd_size = strlen("cmd.exe /c \"\"") + strlen(bat_path) + 3;
    for (int i = 1; i < argc; i++) {
        cmd_size += strlen(argv[i]) + 3;
    }

    char* cmd = (char*)malloc(cmd_size);
    if (!cmd) {
        fprintf(stderr, "qmake-debug-wrapper: malloc failed\n");
        return 1;
    }

    snprintf(cmd, cmd_size, "cmd.exe /c \"\"%s\"", bat_path);
    for (int i = 1; i < argc; i++) {
        if (strchr(argv[i], ' ') || strchr(argv[i], '\t')) {
            strcat(cmd, " \"");
            strcat(cmd, argv[i]);
            strcat(cmd, "\"");
        } else {
            strcat(cmd, " ");
            strcat(cmd, argv[i]);
        }
    }
    strcat(cmd, "\"");

    STARTUPINFOA si;
    memset(&si, 0, sizeof(si));
    si.cb = sizeof(si);

    PROCESS_INFORMATION pi;
    memset(&pi, 0, sizeof(pi));

    if (!CreateProcessA(NULL, cmd, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi)) {
        fprintf(stderr, "qmake-debug-wrapper: CreateProcess failed (%lu)\n", GetLastError());
        free(cmd);
        return 1;
    }

    free(cmd);

    WaitForSingleObject(pi.hProcess, INFINITE);

    DWORD exit_code;
    GetExitCodeProcess(pi.hProcess, &exit_code);

    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);

    return (int)exit_code;
}
"""

def _find_vcvarsall():
    """Find vcvarsall.bat via vswhere or common paths."""
    vswhere_paths = [
        os.path.join(os.environ.get("ProgramFiles(x86)", ""), "Microsoft Visual Studio", "Installer", "vswhere.exe"),
        os.path.join(os.environ.get("ProgramFiles", ""), "Microsoft Visual Studio", "Installer", "vswhere.exe"),
    ]
    vswhere = None
    for p in vswhere_paths:
        if os.path.isfile(p):
            vswhere = p
            break
    if vswhere is None:
        result = subprocess.run("where vswhere.exe", capture_output=True, text=True, shell=True)
        if result.returncode == 0:
            vswhere = result.stdout.strip().splitlines()[0]

    if vswhere:
        result = subprocess.run(
            f'"{vswhere}" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath',
            capture_output=True, text=True, shell=True
        )
        if result.returncode == 0 and result.stdout.strip():
            install_path = result.stdout.strip()
            vcvarsall = os.path.join(install_path, "VC", "Auxiliary", "Build", "vcvarsall.bat")
            if os.path.isfile(vcvarsall):
                return vcvarsall

    for base in [os.environ.get("ProgramFiles", ""), os.environ.get("ProgramFiles(x86)", "")]:
        for edition in ["Community", "Professional", "Enterprise", "BuildTools"]:
            for ver in ["2022", "2019", "2017"]:
                candidate = os.path.join(base, "Microsoft Visual Studio", ver, edition, "VC", "Auxiliary", "Build", "vcvarsall.bat")
                if os.path.isfile(candidate):
                    return candidate
    return None


def _get_msvc_env(arch="x64"):
    """Run vcvarsall.bat and return an enriched environment dict."""
    vcvarsall = _find_vcvarsall()
    if vcvarsall is None:
        print("Error: Could not find vcvarsall.bat. Cannot compile qmake debug wrapper.")
        sys.exit(1)

    cmd = f'cmd.exe /c ""{vcvarsall}" {arch} >nul 2>&1 && set"'
    result = subprocess.run(cmd, capture_output=True, text=True, shell=True)
    if result.returncode != 0:
        print(f"Error: Failed to run vcvarsall.bat ({vcvarsall})")
        print(result.stderr)
        sys.exit(1)

    env = os.environ.copy()
    for line in result.stdout.splitlines():
        if "=" in line:
            key, _, value = line.partition("=")
            env[key] = value
    return env


def compile_qmake_debug_wrapper(output_dir):
    """Compile the qmake debug wrapper executable next to qmake.debug.bat."""
    wrapper_exe = os.path.join(output_dir, "qmake_debug_wrapper.exe")

    if os.path.exists(wrapper_exe):
        print(f"qmake debug wrapper already exists at: {wrapper_exe}")
        return wrapper_exe

    wrapper_src = os.path.join(output_dir, "qmake_debug_wrapper.c")
    with open(wrapper_src, "w") as f:
        f.write(QMAKE_DEBUG_WRAPPER_C)

    # Set up MSVC env for this subprocess only — does not affect the caller's env
    msvc_env = _get_msvc_env("x64")
    cmd = f'cl.exe /nologo /O2 /Fe"{wrapper_exe}" "{wrapper_src}"'
    print(f"Compiling qmake debug wrapper: {cmd}")
    result = subprocess.run(cmd, shell=True, env=msvc_env)
    if result.returncode != 0:
        print("Error: Failed to compile qmake debug wrapper.")
        sys.exit(1)

    # Clean up intermediate files
    if os.path.exists(wrapper_src):
        os.remove(wrapper_src)
    obj_path = wrapper_src.replace('.c', '.obj')
    if os.path.exists(obj_path):
        os.remove(obj_path)

    print(f"Compiled qmake debug wrapper: {wrapper_exe}")
    return wrapper_exe

def find_qmake(directory, is_debug_build):
    env_settings = dict()

    if sys.platform == "win32" and is_debug_build:
        # Find qmake.debug.bat first, then compile a wrapper exe next to it.
        # cxx-qt expects qmake to be a real executable and cannot run .bat files.
        tail = os.path.join("Qt6", "bin", "qmake.debug.bat")
        pattern = f'{directory}/**/{tail}'
        print(f"Looking for qmake at: {pattern}")
        bat_paths = glob.glob(pattern, recursive=True)
        if not bat_paths:
            return (None, None)
        bat_dir = os.path.dirname(bat_paths[0])
        qmake = compile_qmake_debug_wrapper(bat_dir)
    else:
        win_qmake = 'qmake.exe' if sys.platform == "win32" else 'qmake'
        tail = os.path.join("Qt6", "bin", win_qmake)
        pattern = f'{directory}/**/{tail}'
        print(f"Looking for qmake at: {pattern}")
        qmake_paths = glob.glob(pattern, recursive=True)
        if not qmake_paths:
            return (None, None)
        qmake = qmake_paths[0]

    return (qmake, env_settings)

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
    dbgpart = "*-*/debug/" if is_debug_build else "*-*/"

    def find_path_based_on_tail(tail, is_debug_build):
        pattern = f'{installed_dir}/**/{dbgpart}{tail}'
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

    # TODO: handle MacOS
    runtime_tail = os.path.join("bin", "zita-resampler.dll") if sys.platform == "win32" \
           else os.path.join("lib", "libzita-resampler.so") if sys.platform == "linux" \
           else os.path.join("lib", "libzita-resampler.dylib")
    compiletime_tail = os.path.join("lib", "zita-resampler.lib") if sys.platform == "win32" \
           else os.path.join("lib", "libzita-resampler.so") if sys.platform == "linux" \
           else os.path.join("lib", "libzita-resampler.dylib")
    
    runtime = find_path_based_on_tail(runtime_tail, is_debug_build)
    compiletime = find_path_based_on_tail(compiletime_tail, is_debug_build)

    sep = ';' if sys.platform == 'win32' else ':'
    
    # add manually-linked libs (i.e. Catch2)
    print(f"Looking for manual-link folders")
    for dir in glob.glob(f"{installed_dir}/**/{dbgpart}/*/manual-link", recursive=True):
        print(f"Found manual-link folder: {dir}")
        runtime = f'{runtime}{sep}{dir}'

    return (runtime, compiletime)

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
    new_env["CMAKE_PREFIX_PATH"] = os.path.join(args.vcpkg_installed_dir, detect_vcpkg_triplet())
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

    # Find link directories
    # Tell the build where to find link-time and runtime dependencies
    (runtime_dirs, compiletime_dirs) = find_vcpkg_dynlibs_paths(args.vcpkg_installed_dir, is_debug)
    build_env['SHOOP_BUILD_TIME_LINK_DIRS'] = compiletime_dirs
    build_env['SHOOP_RUNTIME_LINK_DIRS'] = runtime_dirs

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