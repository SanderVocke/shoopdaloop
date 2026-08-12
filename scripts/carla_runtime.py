#!/usr/bin/env python3
"""Create and verify a relocatable ShoopDaLoop Carla runtime component."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "third_party" / "carla" / "runtime-lock.json"
LOCK_DATA = json.loads(LOCK.read_text())
LIBRARY_NAMES = {
    "linux": "libcarla_native-plugin.so",
    "windows": "libcarla_native-plugin.dll",
    "macos": "libcarla_native-plugin.dylib",
}
LINUX_PLATFORM_LIBRARIES = set(LOCK_DATA["component"]["linux_platform_libraries"])
HELPER_NAMES = {
    "linux": ("carla-plugin", "carla-plugin-patchbay"),
    "windows": ("carla-plugin.exe", "carla-plugin-patchbay.exe"),
    "macos": ("carla-plugin", "carla-plugin-patchbay"),
}
FORBIDDEN_LIBRARY_PREFIXES = ("libcarla_host-plugin", "libcarla_standalone")
FORBIDDEN_STANDALONE_NAMES = {"carla", "carla.exe", "carla-control", "carla-control.exe"}


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
        path for root in (output / "lib", output / "bin", output / "resources")
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
            if len(fields) >= 3 and fields[1] == "=>" and fields[2] == "not":
                relative = binary.relative_to(output)
                if relative.parts[0] in {"lib", "bin"} or binary.name == "libqxcb.so":
                    raise RuntimeError(
                        f"unresolved Linux dependency for {binary}: {fields[0]}"
                    )
                continue
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
    for root in (output / "lib", output / "bin", output / "resources"):
        for binary in root.rglob("*"):
            if not binary.is_file():
                continue
            probe = subprocess.run(
                ["patchelf", "--print-rpath", str(binary)], text=True, capture_output=True
            )
            if probe.returncode != 0:
                continue
            relative_lib = os.path.relpath(output / "lib", binary.parent)
            component_rpath = "$ORIGIN" if relative_lib == "." else f"$ORIGIN/{relative_lib}"
            original_rpath = probe.stdout.strip()
            rpath = ":".join(filter(None, (original_rpath, component_rpath)))
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
        if source.name.startswith(FORBIDDEN_LIBRARY_PREFIXES):
            continue
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
    extension = ".exe" if args.platform == "windows" else ""
    ui_helper_names = {
        name.removesuffix(extension) if extension else name
        for name in HELPER_NAMES[args.platform]
    }
    required_helpers = [
        name + extension for name in LOCK_DATA["component"]["required_helpers"]
        if name not in ui_helper_names
    ]
    required_helpers.extend(HELPER_NAMES[args.platform])
    for name in required_helpers:
        source = find_one(helper_roots, name)
        copy_file(source, output / "bin" / name, executable=True)

    optional_prefixes = tuple(LOCK_DATA["component"]["optional_helper_prefixes"])
    for root in helper_roots:
        if not root.is_dir():
            continue
        for source in root.rglob("carla-*"):
            if source.is_file() and source.name.startswith(optional_prefixes):
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
    if set(manifest) != {"schema", "platform", "library", "resource_dir", "files"}:
        raise RuntimeError("unexpected Carla component manifest fields")
    if manifest.get("schema") != 1:
        raise RuntimeError("unsupported Carla component manifest")
    if manifest.get("platform") not in LIBRARY_NAMES:
        raise RuntimeError("unknown Carla component platform")
    if platform and manifest.get("platform") != platform:
        raise RuntimeError("Carla component platform mismatch")
    if manifest.get("library") != f"lib/{LIBRARY_NAMES[manifest['platform']]}" or manifest.get("resource_dir") != "resources":
        raise RuntimeError("Carla component has unexpected runtime paths")
    if not isinstance(manifest.get("files"), list):
        raise RuntimeError("Carla component files must be a list")
    entries = manifest["files"]
    paths = []
    for entry in entries:
        if set(entry) != {"path", "sha256", "size", "executable"}:
            raise RuntimeError("unexpected Carla component file fields")
        relative = PurePosixPath(entry["path"])
        if relative.is_absolute() or ".." in relative.parts or relative.as_posix() != entry["path"]:
            raise RuntimeError(f"unsafe Carla component path: {entry['path']}")
        if not isinstance(entry["size"], int) or entry["size"] < 0 or not isinstance(entry["executable"], bool):
            raise RuntimeError(f"invalid Carla component metadata: {entry['path']}")
        if len(entry["sha256"]) != 64 or any(character not in "0123456789abcdef" for character in entry["sha256"]):
            raise RuntimeError(f"invalid Carla component digest: {entry['path']}")
        paths.append(entry["path"])
    if len(paths) != len(set(paths)):
        raise RuntimeError("duplicate Carla component manifest path")
    expected = set(paths) | {"manifest.json"}
    actual = {
        PurePosixPath(path.relative_to(root)).as_posix()
        for path in root.rglob("*")
        if path.is_file()
    }
    symlinks = [path for path in root.rglob("*") if path.is_symlink()]
    if symlinks:
        raise RuntimeError(f"Carla component contains symlinks: {symlinks}")
    if actual != expected:
        raise RuntimeError(
            f"Carla component manifest mismatch; missing={sorted(expected-actual)}, extra={sorted(actual-expected)}"
        )
    for entry in entries:
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
    if (root / "runtime-lock.json").read_bytes() != LOCK.read_bytes():
        raise RuntimeError("Carla component runtime lock differs from the reviewed lock")
    if (root / "licenses" / "README.md").read_bytes() != (ROOT / "third_party" / "carla" / "README.md").read_bytes():
        raise RuntimeError("Carla component review notice differs from the checked-in notice")
    excluded_prefixes = tuple(LOCK_DATA["component"]["excluded_prefixes"])
    forbidden = [
        path for path in actual
        if any(part.startswith(excluded_prefixes) for part in PurePosixPath(path).parts)
        or PurePosixPath(path).name.lower() in FORBIDDEN_STANDALONE_NAMES
    ]
    if forbidden:
        raise RuntimeError(f"Carla component contains excluded wrappers/standalone payloads: {sorted(forbidden)}")
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
