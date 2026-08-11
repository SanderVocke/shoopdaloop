#!/usr/bin/env python3
"""Create and verify a relocatable ShoopDaLoop Carla runtime component."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import stat
import subprocess
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "third_party" / "carla" / "runtime-lock.json"
LIBRARY_NAMES = {
    "linux": "libcarla_native-plugin.so",
    "windows": "libcarla_native-plugin.dll",
    "macos": "libcarla_native-plugin.dylib",
}
LINUX_PLATFORM_LIBRARIES = {
    "ld-linux-x86-64.so.2", "libc.so.6", "libdl.so.2", "libgcc_s.so.1",
    "libm.so.6", "libmvec.so.1", "libpthread.so.0", "librt.so.1",
    "libstdc++.so.6", "libGL.so.1", "libGLX.so.0", "libGLdispatch.so.0",
}
HELPER_NAMES = {
    "linux": ("carla-plugin", "carla-plugin-patchbay"),
    "windows": ("carla-plugin.exe", "carla-plugin-patchbay.exe"),
    "macos": ("carla-plugin", "carla-plugin-patchbay"),
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def copy_file(source: Path, destination: Path, executable: bool = False) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        destination.chmod(destination.stat().st_mode | stat.S_IWUSR)
        destination.unlink()
    shutil.copy2(source.resolve(), destination)
    if executable:
        destination.chmod(destination.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def find_one(roots: list[Path], name: str) -> Path:
    matches = []
    for root in roots:
        if root.is_dir():
            matches.extend(path for path in root.rglob(name) if path.is_file())
    if not matches:
        raise RuntimeError(f"Carla payload is missing {name}")
    matches.sort(key=lambda path: (len(path.parts), str(path)))
    return matches[0]


def bundle_linux_dependencies(output: Path) -> None:
    queue = [
        path for root in (output / "lib", output / "bin")
        for path in root.rglob("*") if path.is_file()
    ]
    inspected: set[Path] = set()
    while queue:
        binary = queue.pop()
        if binary in inspected:
            continue
        inspected.add(binary)
        result = subprocess.run(["ldd", str(binary)], text=True, capture_output=True)
        if result.returncode != 0:
            continue
        for line in result.stdout.splitlines():
            fields = line.strip().split()
            if len(fields) < 3 or fields[1] != "=>" or not fields[2].startswith("/"):
                continue
            name, source = fields[0], Path(fields[2])
            if name in LINUX_PLATFORM_LIBRARIES:
                continue
            destination = output / "lib" / name
            if not destination.exists():
                copy_file(source, destination)
                queue.append(destination)
    patchelf = shutil.which("patchelf")
    if not patchelf:
        raise RuntimeError("patchelf is required to normalize a Linux Carla runtime")
    for root, rpath in ((output / "lib", "$ORIGIN"), (output / "bin", "$ORIGIN/../lib")):
        for binary in root.rglob("*"):
            if not binary.is_file():
                continue
            probe = subprocess.run(["patchelf", "--print-rpath", str(binary)], capture_output=True)
            if probe.returncode == 0:
                binary.chmod(binary.stat().st_mode | stat.S_IWUSR)
                subprocess.run(["patchelf", "--set-rpath", rpath, str(binary)], check=True)


def normalize(args: argparse.Namespace) -> None:
    output = args.output.resolve()
    if output.exists():
        for path in output.rglob("*"):
            path.chmod(path.stat().st_mode | stat.S_IWUSR)
        shutil.rmtree(output)
    (output / "lib").mkdir(parents=True)
    roots = [path.resolve() for path in args.search_root]
    library = find_one(roots, LIBRARY_NAMES[args.platform])
    library_dir = library.parent

    # Keep Carla's own adjacent runtime libraries and styles. System dependency
    # closure normalization remains a platform build responsibility.
    for source in sorted(library_dir.iterdir()):
        if source.is_file() and (
            source.name == library.name
            or source.name.startswith("libcarla_")
            or source.suffix.lower() in {".dll", ".dylib"}
        ):
            copy_file(source, output / "lib" / source.name)
        elif source.is_dir() and source.name in {"styles", "jack"}:
            destination = output / "lib" / source.name
            shutil.copytree(source, destination, symlinks=False)
            for copied in destination.rglob("*"):
                copied.chmod(copied.stat().st_mode | stat.S_IWUSR)

    helper_roots = roots + [library_dir]
    required_helpers = [
        *HELPER_NAMES[args.platform],
        "carla-discovery-native" + (".exe" if args.platform == "windows" else ""),
        "carla-bridge-native" + (".exe" if args.platform == "windows" else ""),
    ]
    for name in required_helpers:
        source = find_one(helper_roots, name)
        copy_file(source, output / "bin" / name, executable=True)

    for root in helper_roots:
        if not root.is_dir():
            continue
        for source in root.rglob("carla-bridge-*"):
            if source.is_file():
                copy_file(source, output / "bin" / source.name, executable=True)

    resource_candidates = []
    for root in roots:
        resource_candidates.extend(
            path for path in root.rglob("resources")
            if path.is_dir() and (path / HELPER_NAMES[args.platform][0]).is_file()
        )
    if not resource_candidates:
        raise RuntimeError("Carla payload has no UI resource directory")
    resource_candidates.sort(key=lambda path: (len(path.parts), str(path)))
    shutil.copytree(resource_candidates[0], output / "resources", symlinks=False)

    if args.platform == "linux":
        bundle_linux_dependencies(output)

    # Carla's frozen external UI resolves libcarla_utils and its dependency
    # closure from the runtime root (the parent of resources), while the host
    # library and discovery helpers use lib/. Keep both layouts explicit.
    for library in (output / "lib").iterdir():
        if library.is_file():
            shutil.copy2(library, output / library.name)

    (output / "licenses").mkdir()
    shutil.copy2(LOCK, output / "runtime-lock.json")
    shutil.copy2(ROOT / "third_party" / "carla" / "README.md", output / "licenses" / "README.md")
    shutil.copy2(args.license.resolve(), output / "licenses" / "COPYING")

    files = []
    for path in sorted(output.rglob("*")):
        if path.is_file() and path.name != "manifest.json":
            relative = PurePosixPath(path.relative_to(output)).as_posix()
            files.append(
                {
                    "path": relative,
                    "sha256": digest(path),
                    "size": path.stat().st_size,
                    "executable": bool(path.stat().st_mode & 0o111),
                }
            )
    manifest = {
        "schema": 1,
        "platform": args.platform,
        "library": f"lib/{LIBRARY_NAMES[args.platform]}",
        "resource_dir": "resources",
        "files": files,
    }
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    verify_component(output, args.platform)


def verify_component(root: Path, platform: str | None = None) -> dict:
    manifest_path = root / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("schema") != 1:
        raise RuntimeError("unsupported Carla component manifest")
    if platform and manifest.get("platform") != platform:
        raise RuntimeError("Carla component platform mismatch")
    expected = {entry["path"] for entry in manifest["files"]} | {"manifest.json"}
    actual = {
        PurePosixPath(path.relative_to(root)).as_posix()
        for path in root.rglob("*")
        if path.is_file()
    }
    if actual != expected:
        raise RuntimeError(
            f"Carla component manifest mismatch; missing={sorted(expected-actual)}, extra={sorted(actual-expected)}"
        )
    for entry in manifest["files"]:
        path = root / entry["path"]
        if path.stat().st_size != entry["size"] or digest(path) != entry["sha256"]:
            raise RuntimeError(f"Carla component checksum mismatch: {entry['path']}")
        if entry["executable"] and path.stat().st_mode & 0o111 == 0:
            raise RuntimeError(f"Carla component lost executable mode: {entry['path']}")
    suffix = ".exe" if manifest["platform"] == "windows" else ""
    required = {
        manifest["library"],
        "runtime-lock.json",
        "licenses/README.md",
        "licenses/COPYING",
        f"bin/carla-discovery-native{suffix}",
        f"bin/carla-bridge-native{suffix}",
        f"resources/{HELPER_NAMES[manifest['platform']][0]}",
        f"resources/{HELPER_NAMES[manifest['platform']][1]}",
    }
    if not required <= actual:
        raise RuntimeError(f"Carla component is incomplete: {sorted(required-actual)}")
    if any("carla.lv2" in path or "carla.vst" in path for path in actual):
        raise RuntimeError("Carla component contains a plugin wrapper")
    return manifest


def verify(args: argparse.Namespace) -> None:
    verify_component(args.component.resolve(), args.platform)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)
    build = commands.add_parser("normalize")
    build.add_argument("--platform", choices=tuple(LIBRARY_NAMES), required=True)
    build.add_argument("--search-root", type=Path, action="append", required=True)
    build.add_argument("--license", type=Path, required=True)
    build.add_argument("--output", type=Path, required=True)
    build.set_defaults(function=normalize)
    check = commands.add_parser("verify")
    check.add_argument("--platform", choices=tuple(LIBRARY_NAMES), required=True)
    check.add_argument("--component", type=Path, required=True)
    check.set_defaults(function=verify)
    return result


def main() -> None:
    args = parser().parse_args()
    args.function(args)


if __name__ == "__main__":
    main()
