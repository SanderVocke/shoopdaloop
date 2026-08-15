#!/usr/bin/env python3

from __future__ import annotations

import argparse
import contextlib
import dataclasses
import hashlib
import http.server
import json
import os
import pathlib
import shutil
import subprocess
import sys
import threading
import time
import tomllib

from wasm_test_report import write_junit

ROOT = pathlib.Path(__file__).resolve().parents[1]
WASM_PACK_VERSION = "0.15.0"
WASM_BINDGEN_VERSION = "0.2.127"
WASM_BINDGEN_TEST_VERSION = "0.3.77"
NODE_MAJOR = 22


class AssetHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Cross-Origin-Resource-Policy", "cross-origin")
        super().end_headers()

    def log_message(self, format, *args):
        pass


@contextlib.contextmanager
def asset_server(directory: pathlib.Path):
    handler = lambda *args, **kwargs: AssetHandler(  # noqa: E731
        *args, directory=str(directory), **kwargs
    )
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
        if thread.is_alive():
            raise RuntimeError("Wasm test asset server did not stop")


def run_checked(command: list[str], *, env=None, cwd=ROOT) -> str:
    result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True)
    output = result.stdout + result.stderr
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}\n{output}")
    return output.strip()


def validate_tools(runtime: str, env: dict[str, str]):
    wasm_pack = run_checked(["wasm-pack", "--version"], env=env)
    if wasm_pack != f"wasm-pack {WASM_PACK_VERSION}":
        raise RuntimeError(f"expected wasm-pack {WASM_PACK_VERSION}, got {wasm_pack!r}")
    node = run_checked(["node", "--version"], env=env).lstrip("v").split(".")
    if int(node[0]) != NODE_MAJOR:
        raise RuntimeError(f"expected Node {NODE_MAJOR}.x, got {'.'.join(node)}")
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    versions = {package["name"]: package["version"] for package in lock["package"]}
    expected = {
        "wasm-bindgen": WASM_BINDGEN_VERSION,
        "wasm-bindgen-test": WASM_BINDGEN_TEST_VERSION,
    }
    for package, version in expected.items():
        if versions.get(package) != version:
            raise RuntimeError(f"expected {package} {version}, got {versions.get(package)!r}")
    run_checked(["rustc", "--version"], env=env)
    targets = run_checked(["rustup", "target", "list", "--installed"], env=env).splitlines()
    if "wasm32-unknown-unknown" not in targets:
        raise RuntimeError("the wasm32-unknown-unknown Rust target is not installed")
    if runtime == "chrome":
        run_checked(["chromedriver", "--version"], env=env)
        browser = next(
            (candidate for candidate in ("google-chrome", "google-chrome-stable", "chromium") if shutil.which(candidate, path=env.get("PATH"))),
            None,
        )
        if browser is None:
            raise RuntimeError("Chrome or Chromium is not available on PATH")
        run_checked([browser, "--version"], env=env)


def discover_packages(selected: set[str] | None, env: dict[str, str]):
    metadata = json.loads(run_checked(["cargo", "metadata", "--no-deps", "--format-version", "1"], env=env))
    packages = []
    for package in metadata["packages"]:
        config = (package.get("metadata") or {}).get("shoop-wasm-test", {})
        if not config.get("enabled", False):
            continue
        if selected and package["name"] not in selected:
            continue
        packages.append(
            {
                "name": package["name"],
                "path": pathlib.Path(package["manifest_path"]).parent,
                "browser_feature": config.get("browser-feature"),
                "no_default_features": config.get("no-default-features", False),
            }
        )
    packages.sort(key=lambda package: package["name"])
    if selected:
        missing = selected - {package["name"] for package in packages}
        if missing:
            raise RuntimeError("unknown or non-Wasm-test packages: " + ", ".join(sorted(missing)))
    if not packages:
        raise RuntimeError("Wasm test package discovery returned no packages")
    return packages


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stage_assets(profile: str, env: dict[str, str]) -> tuple[pathlib.Path, dict]:
    cargo_profile = "release" if profile == "ci" else "debug"
    directory = ROOT / "target" / "wasm-tests" / profile / "assets"
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    run_checked(
        [
            sys.executable,
            "src/rust/shoopdaloop/build_worklet.py",
            "--profile",
            cargo_profile,
            "--output-dir",
            str(directory),
        ],
        env=env,
    )
    sources = {
        "raw_wasm_host.js": ROOT / "src/rust/shoopdaloop/raw_wasm_host.js",
        "audio_worker.js": ROOT / "src/rust/shoopdaloop/audio_worker.js",
        "node_worker_bootstrap.mjs": ROOT / "tests/wasm/node_worker_bootstrap.mjs",
    }
    for name, source in sources.items():
        shutil.copy2(source, directory / name)
    assets = {}
    for path in sorted(directory.iterdir()):
        if path.name == "manifest.json":
            continue
        assets[path.name] = {"bytes": path.stat().st_size, "sha256": sha256(path)}
    manifest = {
        "schema": 1,
        "profile": profile,
        "cargo_profile": cargo_profile,
        "git_head": run_checked(["git", "rev-parse", "HEAD"], env=env),
        "assets": assets,
    }
    (directory / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return directory, manifest


def invoke_package(
    package: dict,
    *,
    runtime: str,
    profile: str,
    env: dict[str, str],
    reports: pathlib.Path,
    timeout: int,
    filters: list[str],
    features: list[str],
):
    command = ["wasm-pack", "test"]
    command += ["--node"] if runtime == "node" else ["--headless", "--chrome"]
    if profile == "ci":
        command.append("--release")
    command.append(str(package["path"]))
    if package["no_default_features"]:
        command.append("--no-default-features")
    package_features = list(features)
    if runtime == "chrome" and package["browser_feature"]:
        package_features.append(package["browser_feature"])
    if package_features:
        command += ["--features", ",".join(sorted(set(package_features)))]
    if filters:
        command += ["--", *filters]

    started = time.monotonic()
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
        returncode = result.returncode
        output = result.stdout
    except subprocess.TimeoutExpired as error:
        returncode = 124
        output = (error.stdout or "") + (error.stderr or "")
        if isinstance(output, bytes):
            output = output.decode(errors="replace")
        output += f"\nWasm package deadline exceeded after {timeout} seconds\n"
    elapsed = time.monotonic() - started
    stem = f"{package['name']}-{runtime}"
    log = reports / f"{stem}.log"
    junit = reports / f"{stem}.xml"
    log.write_text(output)
    parsed = write_junit(
        junit,
        package=package["name"],
        runtime=runtime,
        profile=profile,
        command=command,
        returncode=returncode,
        elapsed_seconds=elapsed,
        output=output,
    )
    valid = (
        parsed.listed > 0
        and not parsed.malformed
        and ((returncode == 0 and parsed.failed == 0) or (returncode != 0 and parsed.failed > 0))
    )
    success = valid and returncode == 0 and parsed.failed == 0
    print(
        f"{package['name']} [{runtime}]: return={returncode} tests={parsed.listed} "
        f"failed={parsed.failed} elapsed={elapsed:.2f}s"
    )
    return {
        "package": package["name"],
        "runtime": runtime,
        "profile": profile,
        "returncode": returncode,
        "elapsed_seconds": elapsed,
        "tests": parsed.listed,
        "failed": parsed.failed,
        "ignored": parsed.ignored,
        "testcases": [dataclasses.asdict(case) for case in parsed.cases],
        "malformed": list(parsed.malformed),
        "success": success,
        "log": str(log.relative_to(ROOT)),
        "junit": str(junit.relative_to(ROOT)),
    }


def synthetic_package_failure(
    package: dict,
    *,
    runtime: str,
    profile: str,
    reports: pathlib.Path,
    message: str,
):
    stem = f"{package['name']}-{runtime}"
    log = reports / f"{stem}.log"
    junit = reports / f"{stem}.xml"
    output = message + "\n"
    log.write_text(output)
    parsed = write_junit(
        junit,
        package=package["name"],
        runtime=runtime,
        profile=profile,
        command=[],
        returncode=124,
        elapsed_seconds=0.0,
        output=output,
    )
    return {
        "package": package["name"],
        "runtime": runtime,
        "profile": profile,
        "command": [],
        "returncode": 124,
        "elapsed_seconds": 0.0,
        "tests": parsed.listed,
        "failed": parsed.failed,
        "ignored": parsed.ignored,
        "testcases": [dataclasses.asdict(case) for case in parsed.cases],
        "malformed": list(parsed.malformed),
        "success": False,
        "log": str(log.relative_to(ROOT)),
        "junit": str(junit.relative_to(ROOT)),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, choices=("node", "chrome"))
    parser.add_argument("--profile", default="dev", choices=("dev", "ci"))
    parser.add_argument("--package", action="append")
    parser.add_argument("--filter", action="append", default=[])
    parser.add_argument("--feature", action="append", default=[])
    parser.add_argument("--package-timeout", type=int, default=600)
    parser.add_argument("--global-timeout", type=int, default=3600)
    args = parser.parse_args()

    env = os.environ.copy()
    env.setdefault("RUSTFLAGS", "-D warnings")
    env.setdefault("WASM_BINDGEN_TEST_TIMEOUT", "60")
    validate_tools(args.runtime, env)
    packages = discover_packages(set(args.package) if args.package else None, env)
    assets, manifest = stage_assets(args.profile, env)
    reports = ROOT / "target" / "wasm-tests" / args.profile / "reports" / args.runtime
    if reports.exists():
        shutil.rmtree(reports)
    reports.mkdir(parents=True)

    server_context = asset_server(assets) if args.runtime == "chrome" else contextlib.nullcontext(None)
    with server_context as base_url:
        test_env = env.copy()
        test_env["SHOOP_WASM_TEST_ASSET_DIR"] = str(assets)
        if base_url:
            test_env["SHOOP_WASM_TEST_ASSET_BASE"] = base_url
        execution_started = time.monotonic()
        results = []
        for package in packages:
            remaining = args.global_timeout - (time.monotonic() - execution_started)
            if remaining <= 0:
                results.append(
                    synthetic_package_failure(
                        package,
                        runtime=args.runtime,
                        profile=args.profile,
                        reports=reports,
                        message=(
                            "Wasm global execution deadline exceeded after "
                            f"{args.global_timeout} seconds"
                        ),
                    )
                )
                continue
            results.append(
                invoke_package(
                    package,
                    runtime=args.runtime,
                    profile=args.profile,
                    env=test_env,
                    reports=reports,
                    timeout=min(args.package_timeout, max(1, int(remaining))),
                    filters=args.filter,
                    features=args.feature,
                )
            )

    summary = {
        "schema": 1,
        "runtime": args.runtime,
        "profile": args.profile,
        "filters": args.filter,
        "features": args.feature,
        "package_timeout": args.package_timeout,
        "global_timeout": args.global_timeout,
        "asset_manifest": manifest,
        "packages": results,
        "success": all(result["success"] for result in results),
    }
    (reports / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(f"reports: {reports.relative_to(ROOT)}")
    return 0 if summary["success"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
