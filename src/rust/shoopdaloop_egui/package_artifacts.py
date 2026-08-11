#!/usr/bin/env python3
"""Package and verify standalone ShoopDaLoop egui CI artifacts."""

from __future__ import annotations

import argparse
import base64
import plistlib
import re
import shutil
import stat
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

from build_single_file_app import build_single_file

PACKAGE = Path(__file__).resolve().parent
ROOT = PACKAGE.parents[2]
ARCHIVE_ROOT = "shoopdaloop-egui"
PROFILES = ("debug", "release")
NATIVE_PLATFORMS = ("linux", "windows", "macos")
ROBOTO_FILES = (
    "LICENSE.txt",
    "README.md",
    "Roboto-Regular.ttf",
    "Roboto-Italic.ttf",
    "Roboto-Bold.ttf",
    "Roboto-BoldItalic.ttf",
)
WEB_REQUIRED_FILES = (
    "index.html",
    "audio_worklet.js",
    "generated/shoop_audio_worklet.wasm",
    *(f"roboto/{name}" for name in ROBOTO_FILES),
)


def artifact_stem(platform: str, arch: str, profile: str) -> str:
    return f"shoopdaloop-egui-{platform}-{arch}-{profile}"


def copy_metadata(destination: Path) -> None:
    shutil.copy2(PACKAGE / "README.md", destination / "README.md")
    shutil.copy2(ROOT / "LICENSE", destination / "LICENSE")


def executable_name(platform: str) -> str:
    return "shoopdaloop_egui.exe" if platform == "windows" else "shoopdaloop_egui"


def create_native_stage(platform: str, binary: Path, stage: Path) -> None:
    root = stage / ARCHIVE_ROOT
    root.mkdir()
    copy_metadata(root)
    shutil.copytree(ROOT / "resources" / "fonts" / "roboto", root / "roboto")

    if platform == "macos":
        contents = root / "ShoopDaLoop egui.app" / "Contents"
        executable_dir = contents / "MacOS"
        resources = contents / "Resources"
        executable_dir.mkdir(parents=True)
        resources.mkdir()
        target = executable_dir / executable_name(platform)
        shutil.copy2(binary, target)
        target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        shutil.copy2(ROOT / "resources" / "iconset" / "icon.icns", resources / "icon.icns")
        plist = {
            "CFBundleDisplayName": "ShoopDaLoop egui",
            "CFBundleExecutable": executable_name(platform),
            "CFBundleIconFile": "icon.icns",
            "CFBundleIdentifier": "org.shoopdaloop.egui",
            "CFBundleName": "ShoopDaLoop egui",
            "CFBundlePackageType": "APPL",
            "CFBundleVersion": "0",
            "LSMinimumSystemVersion": "11.0",
            "NSHighResolutionCapable": True,
        }
        with (contents / "Info.plist").open("wb") as handle:
            plistlib.dump(plist, handle, sort_keys=True)
    else:
        target = root / executable_name(platform)
        shutil.copy2(binary, target)
        if platform == "linux":
            target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def write_tar(source: Path, output: Path) -> None:
    with tarfile.open(output, "w:gz", format=tarfile.PAX_FORMAT) as archive:
        archive.add(source, arcname=source.name, recursive=True)


def write_zip(source: Path, output: Path) -> None:
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(source.rglob("*")):
            relative = PurePosixPath(source.name) / path.relative_to(source)
            if path.is_dir():
                continue
            info = zipfile.ZipInfo.from_file(path, str(relative))
            info.compress_type = zipfile.ZIP_DEFLATED
            with path.open("rb") as handle:
                archive.writestr(info, handle.read())


def package_native(args: argparse.Namespace) -> list[Path]:
    binary = args.binary.resolve()
    if not binary.is_file():
        raise RuntimeError(f"native executable does not exist: {binary}")
    expected_name = executable_name(args.platform)
    if binary.name != expected_name:
        raise RuntimeError(f"expected executable named {expected_name}, got {binary.name}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    stem = artifact_stem(args.platform, args.arch, args.profile)
    suffix = ".zip" if args.platform == "windows" else ".tar.gz"
    output = args.output_dir / f"{stem}{suffix}"
    output.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="shoop-egui-package-") as temporary:
        stage = Path(temporary)
        create_native_stage(args.platform, binary, stage)
        if args.platform == "windows":
            write_zip(stage / ARCHIVE_ROOT, output)
        else:
            write_tar(stage / ARCHIVE_ROOT, output)
    verify_native(output, args.platform)
    return [output]


def find_one(dist: Path, pattern: str) -> Path:
    matches = sorted(dist.glob(pattern))
    if len(matches) != 1:
        names = ", ".join(path.name for path in matches) or "none"
        raise RuntimeError(f"expected one {pattern} in {dist}, found: {names}")
    return matches[0]


def package_web(args: argparse.Namespace) -> list[Path]:
    dist = args.dist.resolve()
    for relative in WEB_REQUIRED_FILES:
        if not (dist / relative).is_file():
            raise RuntimeError(f"hosted web bundle is missing {relative}")
    glue = find_one(dist, "shoopdaloop_egui-*.js")
    wasm = find_one(dist, "shoopdaloop_egui-*_bg.wasm")
    allowed = [dist / relative for relative in WEB_REQUIRED_FILES] + [glue, wasm]

    args.output_dir.mkdir(parents=True, exist_ok=True)
    stem = artifact_stem("web", "wasm32", args.profile)
    bundle = args.output_dir / f"{stem}.zip"
    html = args.output_dir / f"{stem}.html"
    bundle.unlink(missing_ok=True)
    html.unlink(missing_ok=True)
    build_single_file(dist, html)

    with tempfile.TemporaryDirectory(prefix="shoop-egui-web-") as temporary:
        root = Path(temporary) / ARCHIVE_ROOT
        for source in allowed:
            relative = source.relative_to(dist)
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        write_zip(root, bundle)

    verify_web(bundle, html)
    return [bundle, html]


def archive_names(path: Path) -> tuple[set[str], dict[str, int]]:
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            files = {name for name in archive.namelist() if not name.endswith("/")}
            modes = {
                info.filename: (info.external_attr >> 16) & 0o777
                for info in archive.infolist()
                if not info.is_dir()
            }
            return files, modes
    with tarfile.open(path, "r:gz") as archive:
        files = {member.name for member in archive.getmembers() if member.isfile()}
        modes = {
            member.name: member.mode & 0o777
            for member in archive.getmembers()
            if member.isfile()
        }
        return files, modes


def archive_file(path: Path, name: str) -> bytes:
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            return archive.read(name)
    with tarfile.open(path, "r:gz") as archive:
        extracted = archive.extractfile(name)
        if extracted is None:
            raise RuntimeError(f"archive entry is unavailable: {name}")
        return extracted.read()


def require_click_assets(binary: bytes, artifact: str) -> None:
    missing = [
        name
        for name in (b"click_high", b"click_low", b"shaker_primary", b"shaker_secondary")
        if name not in binary
    ]
    if missing:
        raise RuntimeError(f"{artifact} is missing embedded click assets: {missing}")


def verify_native(path: Path, platform: str) -> None:
    names, modes = archive_names(path)
    root = f"{ARCHIVE_ROOT}/"
    metadata = {f"{root}README.md", f"{root}LICENSE"}
    fonts = {f"{root}roboto/{name}" for name in ROBOTO_FILES}
    if platform == "macos":
        app = f"{root}ShoopDaLoop egui.app/Contents/"
        required = metadata | fonts | {
            f"{app}Info.plist",
            f"{app}MacOS/shoopdaloop_egui",
            f"{app}Resources/icon.icns",
        }
        executable = f"{app}MacOS/shoopdaloop_egui"
    else:
        executable = f"{root}{executable_name(platform)}"
        required = metadata | fonts | {executable}
    if names != required:
        raise RuntimeError(
            f"unexpected {platform} archive manifest; missing={sorted(required - names)}, "
            f"extra={sorted(names - required)}"
        )
    if platform != "windows" and modes.get(executable, 0) & 0o111 == 0:
        raise RuntimeError(f"archive executable has no execute bit: {executable}")
    require_click_assets(archive_file(path, executable), "native executable")


def verify_web(bundle: Path, html: Path) -> None:
    names, _ = archive_names(bundle)
    root = f"{ARCHIVE_ROOT}/"
    fixed = {f"{root}{relative}" for relative in WEB_REQUIRED_FILES}
    glue = [name for name in names if name.startswith(f"{root}shoopdaloop_egui-") and name.endswith(".js")]
    wasm = [name for name in names if name.startswith(f"{root}shoopdaloop_egui-") and name.endswith("_bg.wasm")]
    required = fixed | set(glue) | set(wasm)
    if len(glue) != 1 or len(wasm) != 1 or names != required:
        raise RuntimeError(f"unexpected hosted web archive manifest: {sorted(names)}")
    if any("preview" in name.lower() for name in names):
        raise RuntimeError("hosted web archive contains a preview-named file")
    require_click_assets(archive_file(bundle, wasm[0]), "hosted application Wasm")
    text = html.read_text(encoding="utf-8")
    if "TrunkApplicationStarted" not in text or "shoopWasmBytes" not in text:
        raise RuntimeError("self-contained HTML does not contain the embedded application")
    if "enable_midi" not in text or "requestMIDIAccess" not in text:
        raise RuntimeError("self-contained HTML does not contain Web MIDI access")
    if (
        "shoopEmbeddedAudioWorklet" not in text
        or "shoopAudioWorkletWasmBytes" not in text
    ):
        raise RuntimeError("self-contained HTML does not contain embedded browser audio")
    external_fonts = (
        f'url("./roboto/{name}")'
        for name in ROBOTO_FILES
        if name.endswith(".ttf")
    )
    if any(url in text for url in external_fonts):
        raise RuntimeError("self-contained HTML contains an external Roboto font URL")
    if text.count('url("data:font/ttf;base64,') != 4:
        raise RuntimeError("self-contained HTML does not contain every embedded Roboto font face")
    application_binary = None
    for variable in ("shoopWasmBinary", "shoopAudioWorkletBinary"):
        match = re.search(rf'const {variable} = atob\("([A-Za-z0-9+/=]+)"\);', text)
        if not match:
            raise RuntimeError(f"self-contained HTML is missing {variable}")
        binary = base64.b64decode(match.group(1), validate=True)
        if not binary.startswith(b"\0asm"):
            raise RuntimeError(f"self-contained HTML has invalid {variable}")
        if variable == "shoopWasmBinary":
            application_binary = binary
    require_click_assets(application_binary or b"", "self-contained application Wasm")
    worklet = re.search(
        r'const shoopAudioWorkletModuleUrl = "data:text/javascript;base64,([A-Za-z0-9+/=]+)";',
        text,
    )
    if not worklet or b"registerProcessor" not in base64.b64decode(
        worklet.group(1), validate=True
    ):
        raise RuntimeError("self-contained HTML has an invalid AudioWorklet script")
    if html.stat().st_size <= 0:
        raise RuntimeError("self-contained HTML is empty")


def verify(args: argparse.Namespace) -> list[Path]:
    if args.platform == "web":
        if args.html is None:
            raise RuntimeError("--html is required when verifying a web artifact")
        verify_web(args.artifact, args.html)
        return [args.artifact, args.html]
    verify_native(args.artifact, args.platform)
    return [args.artifact]


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)

    native = subcommands.add_parser("native")
    native.add_argument("--platform", choices=NATIVE_PLATFORMS, required=True)
    native.add_argument("--arch", required=True)
    native.add_argument("--profile", choices=PROFILES, required=True)
    native.add_argument("--binary", type=Path, required=True)
    native.add_argument("--output-dir", type=Path, required=True)
    native.set_defaults(function=package_native)

    web = subcommands.add_parser("web")
    web.add_argument("--profile", choices=PROFILES, required=True)
    web.add_argument("--dist", type=Path, required=True)
    web.add_argument("--output-dir", type=Path, required=True)
    web.set_defaults(function=package_web)

    check = subcommands.add_parser("verify")
    check.add_argument("--platform", choices=(*NATIVE_PLATFORMS, "web"), required=True)
    check.add_argument("--artifact", type=Path, required=True)
    check.add_argument("--html", type=Path)
    check.set_defaults(function=verify)
    return result


def main() -> None:
    args = parser().parse_args()
    outputs = args.function(args)
    for output in outputs:
        print(output.resolve())


if __name__ == "__main__":
    main()
