#!/usr/bin/env python3
"""Package and verify standalone ShoopDaLoop CI artifacts."""

from __future__ import annotations

import argparse
import base64
import hashlib
import plistlib
import json
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

from build_single_file_app import build_single_file

PACKAGE = Path(__file__).resolve().parent
ROOT = PACKAGE.parents[2]
ARCHIVE_ROOT = "shoopdaloop"
SOURCE_TREE_MARKER = "SHOOP_SRC_TREE"
PROFILES = ("debug", "release")
NATIVE_PLATFORMS = ("linux", "windows", "macos")
APPLICATION_ICON = ROOT / "resources" / "iconset" / "icon.png"
BUILTINS = ROOT / "resources" / "builtins"
sys.path.insert(0, str(ROOT / "scripts"))
from carla_runtime import verify_component  # noqa: E402
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
    "icon.png",
    "raw_wasm_host.js",
    "audio_worklet.js",
    "audio_worker.js",
    "worker_fixture_contract.js",
    "generated/shoop_audio_worklet.wasm",
    *(f"roboto/{name}" for name in ROBOTO_FILES),
)


def artifact_stem(platform: str, arch: str, profile: str) -> str:
    return f"shoopdaloop-{platform}-{arch}-{profile}"


def copy_metadata(destination: Path) -> None:
    shutil.copy2(PACKAGE / "README.md", destination / "README.md")
    shutil.copy2(ROOT / "LICENSE", destination / "LICENSE")


def executable_name(platform: str) -> str:
    return "shoopdaloop.exe" if platform == "windows" else "shoopdaloop"


def stage_carla_runtime(platform: str, component: Path, root: Path) -> None:
    verify_component(component, platform)
    if platform == "macos":
        destination = root / "ShoopDaLoop.app" / "Contents" / "Frameworks" / "carla-runtime"
    else:
        destination = root / "carla-runtime"
    shutil.copytree(component, destination)


def create_native_stage(platform: str, binary: Path, carla_runtime: Path, stage: Path) -> None:
    root = stage / ARCHIVE_ROOT
    root.mkdir()
    copy_metadata(root)
    shutil.copytree(ROOT / "resources" / "fonts" / "roboto", root / "roboto")

    if platform == "macos":
        contents = root / "ShoopDaLoop.app" / "Contents"
        executable_dir = contents / "MacOS"
        resources = contents / "Resources"
        executable_dir.mkdir(parents=True)
        resources.mkdir()
        target = executable_dir / executable_name(platform)
        shutil.copy2(binary, target)
        target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        shutil.copy2(ROOT / "resources" / "iconset" / "icon.icns", resources / "icon.icns")
        shutil.copytree(BUILTINS, resources / "builtins")
        plist = {
            "CFBundleDisplayName": "ShoopDaLoop",
            "CFBundleExecutable": executable_name(platform),
            "CFBundleIconFile": "icon.icns",
            "CFBundleIdentifier": "org.shoopdaloop.app",
            "CFBundleName": "ShoopDaLoop",
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
        shutil.copytree(BUILTINS, root / "builtins")
    stage_carla_runtime(platform, carla_runtime, root)


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
    with tempfile.TemporaryDirectory(prefix="shoop-package-") as temporary:
        stage = Path(temporary)
        create_native_stage(args.platform, binary, args.carla_runtime.resolve(), stage)
        staged_executable = (
            stage / ARCHIVE_ROOT / "ShoopDaLoop.app" / "Contents" / "MacOS" / "shoopdaloop"
            if args.platform == "macos"
            else stage / ARCHIVE_ROOT / executable_name(args.platform)
        )
        subprocess.run(
            [str(staged_executable), "--probe-builtins"],
            cwd=stage,
            check=True,
        )
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
    glue = find_one(dist, "shoopdaloop-*.js")
    wasm = find_one(dist, "shoopdaloop-*_bg.wasm")
    builtins = sorted((dist / "builtins").rglob("*"))
    builtins = [path for path in builtins if path.is_file()]
    if not builtins or not (dist / "builtins" / "catalog.json").is_file():
        raise RuntimeError("hosted web bundle is missing the external built-ins tree")
    allowed = [dist / relative for relative in WEB_REQUIRED_FILES] + [glue, wasm] + builtins

    args.output_dir.mkdir(parents=True, exist_ok=True)
    stem = artifact_stem("web", "wasm32", args.profile)
    bundle = args.output_dir / f"{stem}.zip"
    html = args.output_dir / f"{stem}.html"
    bundle.unlink(missing_ok=True)
    html.unlink(missing_ok=True)
    build_single_file(dist, html)

    with tempfile.TemporaryDirectory(prefix="shoop-web-") as temporary:
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


def archive_payloads(path: Path) -> dict[str, bytes]:
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            return {
                info.filename: archive.read(info)
                for info in archive.infolist()
                if not info.is_dir()
            }
    with tarfile.open(path, "r:gz") as archive:
        result = {}
        for member in archive.getmembers():
            if not member.isfile():
                continue
            extracted = archive.extractfile(member)
            if extracted is None:
                raise RuntimeError(f"archive entry is unavailable: {member.name}")
            result[member.name] = extracted.read()
        return result


def builtin_payloads(prefix: str) -> dict[str, bytes]:
    return {
        f"{prefix}{PurePosixPath(path.relative_to(BUILTINS))}": path.read_bytes()
        for path in sorted(BUILTINS.rglob("*"))
        if path.is_file()
    }


def verify_builtins(payloads: dict[str, bytes], prefix: str) -> set[str]:
    expected = builtin_payloads(prefix)
    actual = {name: payload for name, payload in payloads.items() if name.startswith(prefix)}
    if actual != expected:
        raise RuntimeError(
            f"packaged built-ins differ from the source tree; "
            f"missing={sorted(set(expected) - set(actual))}, "
            f"extra={sorted(set(actual) - set(expected))}"
        )
    catalog = json.loads(expected[f"{prefix}catalog.json"].decode("utf-8"))
    records = {record["path"]: record for record in catalog["files"]}
    for name, payload in expected.items():
        relative = name.removeprefix(prefix)
        if relative == "catalog.json":
            continue
        record = records.get(relative)
        if record is None:
            raise RuntimeError(f"built-ins catalog omits {relative}")
        if record["bytes"] != len(payload) or record["sha256"] != hashlib.sha256(payload).hexdigest():
            raise RuntimeError(f"built-ins catalog checksum mismatch: {relative}")
    if set(records) != {name.removeprefix(prefix) for name in expected if not name.endswith("catalog.json")}:
        raise RuntimeError("built-ins catalog has stale or extra records")
    return set(expected)


def reject_application_script_payload(binary: bytes, artifact: str) -> None:
    present = []
    for path in sorted(BUILTINS.rglob("*.lua")):
        marker = next(
            (line for line in path.read_bytes().splitlines() if len(line) >= 48),
            path.read_bytes()[:64],
        )
        if marker and marker in binary:
            present.append(path.name)
    if present:
        raise RuntimeError(f"{artifact} contains compiled application scripts: {present}")


def require_application_icon(binary: bytes, artifact: str) -> None:
    if APPLICATION_ICON.read_bytes() not in binary:
        raise RuntimeError(f"{artifact} is missing the embedded application icon")


def require_click_assets(binary: bytes, artifact: str) -> None:
    missing = [
        name
        for name in (b"click_high", b"click_low", b"shaker_primary", b"shaker_secondary")
        if name not in binary
    ]
    if missing:
        raise RuntimeError(f"{artifact} is missing embedded click assets: {missing}")


def require_native_architecture(binary: bytes, platform: str, artifact: str) -> None:
    if platform == "linux":
        valid = (
            len(binary) >= 20
            and binary[:6] == b"\x7fELF\x02\x01"
            and int.from_bytes(binary[18:20], "little") == 62
        )
    elif platform == "windows":
        pe_offset = int.from_bytes(binary[0x3C:0x40], "little") if len(binary) >= 0x40 else 0
        valid = (
            binary[:2] == b"MZ"
            and pe_offset + 6 <= len(binary)
            and binary[pe_offset : pe_offset + 4] == b"PE\0\0"
            and int.from_bytes(binary[pe_offset + 4 : pe_offset + 6], "little") == 0x8664
        )
    else:
        arm64 = 0x0100000C
        magic = binary[:4]
        valid = magic == b"\xcf\xfa\xed\xfe" and int.from_bytes(binary[4:8], "little") == arm64
        if magic in {b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"} and len(binary) >= 8:
            count = int.from_bytes(binary[4:8], "big")
            stride = 32 if magic == b"\xca\xfe\xba\xbf" else 20
            valid = any(
                8 + stride * (index + 1) <= len(binary)
                and int.from_bytes(binary[8 + stride * index : 12 + stride * index], "big") == arm64
                for index in range(count)
            )
    if not valid:
        raise RuntimeError(f"{artifact} has the wrong {platform} architecture")


def reject_carla_native_payload(binary: bytes, artifact: str) -> None:
    forbidden = (
        b"carla_get_native_rack_plugin",
        b"carla_get_native_patchbay_plugin",
        b"libcarla_native-plugin",
        b"SHOOP_CARLA_NATIVE_LIBRARY",
        b"SHOOP_CARLA_RESOURCE_DIR",
    )
    present = [value.decode("ascii") for value in forbidden if value in binary]
    if present:
        raise RuntimeError(f"{artifact} contains Carla Native payload markers: {present}")


def carla_archive_path(platform: str, relative: str) -> str:
    root = f"{ARCHIVE_ROOT}/"
    if platform == "macos":
        return f"{root}ShoopDaLoop.app/Contents/Frameworks/carla-runtime/{relative}"
    return f"{root}carla-runtime/{relative}"


def verify_native(path: Path, platform: str) -> None:
    names, modes = archive_names(path)
    payloads = archive_payloads(path)
    root = f"{ARCHIVE_ROOT}/"
    metadata = {f"{root}README.md", f"{root}LICENSE"}
    fonts = {f"{root}roboto/{name}" for name in ROBOTO_FILES}
    if platform == "macos":
        app = f"{root}ShoopDaLoop.app/Contents/"
        required = metadata | fonts | {
            f"{app}Info.plist",
            f"{app}MacOS/shoopdaloop",
            f"{app}Resources/icon.icns",
        }
        executable = f"{app}MacOS/shoopdaloop"
        source_tree_marker = f"{app}MacOS/{SOURCE_TREE_MARKER}"
        builtins_prefix = f"{app}Resources/builtins/"
        component_manifest_path = f"{app}Frameworks/carla-runtime/manifest.json"
    else:
        executable = f"{root}{executable_name(platform)}"
        source_tree_marker = f"{root}{SOURCE_TREE_MARKER}"
        required = metadata | fonts | {executable}
        builtins_prefix = f"{root}builtins/"
        component_manifest_path = f"{root}carla-runtime/manifest.json"
    if source_tree_marker in names:
        raise RuntimeError("native archive contains a source-tree marker")
    try:
        component_manifest = json.loads(
            payloads[component_manifest_path].decode("utf-8")
        )
    except (KeyError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("native archive has no valid Carla component manifest") from error
    if component_manifest.get("platform") != platform:
        raise RuntimeError("native archive contains the wrong Carla component platform")
    lock_path = carla_archive_path(platform, "runtime-lock.json")
    notice_path = carla_archive_path(platform, "licenses/README.md")
    if payloads.get(lock_path) != (ROOT / "third_party" / "carla" / "runtime-lock.json").read_bytes():
        raise RuntimeError("native archive contains an unreviewed Carla runtime lock")
    if payloads.get(notice_path) != (ROOT / "third_party" / "carla" / "README.md").read_bytes():
        raise RuntimeError("native archive contains an unreviewed Carla notice")
    required.add(component_manifest_path)
    required.update(verify_builtins(payloads, builtins_prefix))
    for entry in component_manifest.get("files", []):
        archived = carla_archive_path(platform, entry["path"])
        required.add(archived)
        payload = payloads[archived]
        if len(payload) != entry["size"] or hashlib.sha256(payload).hexdigest() != entry["sha256"]:
            raise RuntimeError(f"packaged Carla checksum mismatch: {archived}")
        if platform != "windows" and entry["executable"] and modes.get(archived, 0) & 0o111 == 0:
            raise RuntimeError(f"packaged Carla helper has no execute bit: {archived}")
    if names != required:
        raise RuntimeError(
            f"unexpected {platform} archive manifest; missing={sorted(required - names)}, "
            f"extra={sorted(names - required)}"
        )
    if platform != "windows" and modes.get(executable, 0) & 0o111 == 0:
        raise RuntimeError(f"archive executable has no execute bit: {executable}")
    binary = payloads[executable]
    require_native_architecture(binary, platform, "native executable")
    carla_library = payloads[carla_archive_path(platform, component_manifest["library"])]
    require_native_architecture(carla_library, platform, "Carla Native library")
    require_application_icon(binary, "native executable")
    require_click_assets(binary, "native executable")
    reject_application_script_payload(binary, "native executable")


def verify_web(bundle: Path, html: Path) -> None:
    names, _ = archive_names(bundle)
    root = f"{ARCHIVE_ROOT}/"
    fixed = {f"{root}{relative}" for relative in WEB_REQUIRED_FILES}
    glue = [name for name in names if name.startswith(f"{root}shoopdaloop-") and name.endswith(".js")]
    wasm = [name for name in names if name.startswith(f"{root}shoopdaloop-") and name.endswith("_bg.wasm")]
    payloads = archive_payloads(bundle)
    required = fixed | set(glue) | set(wasm)
    required.update(verify_builtins(payloads, f"{root}builtins/"))
    if len(glue) != 1 or len(wasm) != 1 or names != required:
        raise RuntimeError(f"unexpected hosted web archive manifest: {sorted(names)}")
    if any("preview" in name.lower() for name in names):
        raise RuntimeError("hosted web archive contains a preview-named file")
    icon = archive_file(bundle, f"{root}icon.png")
    if icon != APPLICATION_ICON.read_bytes():
        raise RuntimeError("hosted web archive contains the wrong application icon")
    hosted_application = archive_file(bundle, wasm[0])
    hosted_worklet = archive_file(bundle, f"{root}generated/shoop_audio_worklet.wasm")
    require_click_assets(hosted_application, "hosted application Wasm")
    reject_carla_native_payload(hosted_application, "hosted application Wasm")
    reject_application_script_payload(hosted_application, "hosted application Wasm")
    reject_carla_native_payload(hosted_worklet, "hosted AudioWorklet Wasm")
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
    embedded_icon = re.search(
        r'href="data:image/png;base64,([A-Za-z0-9+/=]+)"', text
    )
    if not embedded_icon or base64.b64decode(
        embedded_icon.group(1), validate=True
    ) != APPLICATION_ICON.read_bytes():
        raise RuntimeError("self-contained HTML does not contain the application icon")
    application_binary = None
    for variable in ("shoopWasmBinary", "shoopAudioWorkletBinary"):
        match = re.search(rf'const {variable} = atob\("([A-Za-z0-9+/=]+)"\);', text)
        if not match:
            raise RuntimeError(f"self-contained HTML is missing {variable}")
        binary = base64.b64decode(match.group(1), validate=True)
        if not binary.startswith(b"\0asm"):
            raise RuntimeError(f"self-contained HTML has invalid {variable}")
        reject_carla_native_payload(binary, f"self-contained {variable}")
        if variable == "shoopWasmBinary":
            application_binary = binary
    require_click_assets(application_binary or b"", "single-file core application Wasm")
    reject_application_script_payload(
        application_binary or b"", "single-file core application Wasm"
    )
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
    native.add_argument("--carla-runtime", type=Path, required=True)
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
