#!/usr/bin/env python3
"""Run the independent, non-isolated rt_runtime AudioWorklet and native replays."""
from __future__ import annotations

import argparse
import contextlib
import functools
import gzip
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/rt_runtime_host"
ASSETS = ROOT / "target/rt-runtime-host"
REPORTS = ASSETS / "reports"


class Handler(SimpleHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/journal":
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", "0"))
        if not 0 < length <= 64 * 1024 * 1024:
            self.send_error(413)
            return
        data = self.rfile.read(length)
        if len(data) != length or data[:4] != b"RTJ1":
            self.send_error(400)
            return
        (REPORTS / "journal.bin").write_bytes(data)
        self.send_response(204)
        self.end_headers()

    def log_message(self, *_args):
        pass


@contextlib.contextmanager
def server():
    # Deliberately not the repository Wasm runner's COOP/COEP asset handler.
    instance = ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(ASSETS)))
    thread = threading.Thread(target=instance.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{instance.server_port}"
    finally:
        instance.shutdown()
        instance.server_close()
        thread.join(timeout=5)
        if thread.is_alive():
            raise RuntimeError("fixture server did not stop")


def command(args, log):
    result = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log.write_text(result.stdout)
    if result.returncode:
        raise RuntimeError(f"command failed: {' '.join(map(str, args))}\n{result.stdout[-12000:]}")
    return result.stdout


def http(method, url, data=None):
    request = urllib.request.Request(url, data=None if data is None else json.dumps(data).encode(), method=method,
                                     headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=360) as response:
            result = json.loads(response.read())
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error
    value = result.get("value", result)
    if isinstance(value, dict) and "error" in value:
        raise RuntimeError(json.dumps(value))
    return value


def browser(url):
    chrome = os.environ.get("CHROME_BIN") or shutil.which("chromium") or shutil.which("google-chrome")
    if not chrome:
        raise RuntimeError("Chromium/Chrome is required")
    version = subprocess.check_output([chrome, "--version"], text=True).strip()
    driver_version = subprocess.check_output(["chromedriver", "--version"], text=True).strip()
    if not any(pin in version for pin in ("147.0.7727.117", "147.0.7727.137")) or not any(pin in driver_version for pin in ("147.0.7727.117", "147.0.7727.137")):
        raise RuntimeError(f"unsupported browser tools: {version}; {driver_version}")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    with tempfile.TemporaryDirectory(prefix="rt-runtime-chrome-") as profile, (REPORTS / "chromedriver.log").open("w") as log:
        driver = subprocess.Popen(["chromedriver", f"--port={port}", "--verbose"], stdout=log, stderr=subprocess.STDOUT)
        base = f"http://127.0.0.1:{port}"
        session = None
        try:
            for _ in range(100):
                if driver.poll() is not None:
                    raise RuntimeError("ChromeDriver exited before startup")
                try:
                    http("GET", base + "/status")
                    break
                except urllib.error.URLError:
                    time.sleep(0.05)
            else:
                raise RuntimeError("ChromeDriver startup timed out")
            created = http("POST", base + "/session", {"capabilities": {"alwaysMatch": {
                "browserName": "chrome", "goog:loggingPrefs": {"browser": "ALL"}, "goog:chromeOptions": {"binary": chrome,
                    "args": ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage", "--autoplay-policy=no-user-gesture-required", "--mute-audio", f"--user-data-dir={profile}"]}}}})
            session = created["sessionId"]
            endpoint = base + "/session/" + session
            http("POST", endpoint + "/timeouts", {"script": 300000, "pageLoad": 30000})
            http("POST", endpoint + "/url", {"url": url + "/index.html"})
            result = http("POST", endpoint + "/execute/async", {"script": "const done = arguments[0]; import('./main.js').then(m => m.run()).then(done, e => done({failure: String(e.stack || e)}));", "args": []})
            logs = http("POST", endpoint + "/log", {"type": "browser"})
            (REPORTS / "browser-console.json").write_text(json.dumps(logs, indent=2))
            (REPORTS / "browser.json").write_text(json.dumps(result, indent=2))
            if not result.get("passed"):
                raise RuntimeError(result.get("failure", "missing browser success evidence"))
            result["tools"] = {"chrome": version, "chromedriver": driver_version}
            return result
        finally:
            if session:
                try:
                    http("DELETE", base + "/session/" + session)
                except Exception:
                    pass
            driver.terminate()
            try:
                driver.wait(timeout=10)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="Reuse existing native/Wasm fixture artifacts")
    parser.add_argument("--native-only", action="store_true", help="Replay the checked-in real-worklet journal without browser tools")
    parser.add_argument("--profile", choices=("debug", "release"), default="release", help="Native replay build profile")
    args = parser.parse_args()
    REPORTS.mkdir(parents=True, exist_ok=True)
    (REPORTS / "journal.bin").unlink(missing_ok=True)
    manifest = str(FIXTURE / "Cargo.toml")
    if not args.no_build:
        command(["cargo", "build", "--manifest-path", manifest, "--locked"] + (["--release"] if args.profile == "release" else []), REPORTS / "native-build.log")
        if not args.native_only:
            command(["wasm-pack", "build", str(FIXTURE), "--target", "web", "--release", "--out-dir", str(ASSETS / "pkg"), "--out-name", "fixture", "--no-opt"], REPORTS / "wasm-build.log")
    journal = REPORTS / "journal.bin"
    if args.native_only:
        data = gzip.decompress((FIXTURE / "data/offline.rtj.gz").read_bytes())
        digest = hashlib.sha256(data).hexdigest()
        if digest != "fb8ecf1fae2cb7ac7246bbb836d2ed7670f9d1341b8c005ab3d2565f07ee6237":
            raise RuntimeError("checked-in browser journal digest differs")
        journal.write_bytes(data)
        reference = json.loads((ROOT / "docs/rt_runtime/evidence/hosts-stage6.json").read_text())["reports"][0]
        result = {"passed": True, "browser": "not run; replaying checked-in real-worklet evidence", "golden_sha256": digest, "profile": args.profile}
    else:
        for source in (FIXTURE / "web").glob("*.js"):
            shutil.copy2(source, ASSETS / source.name)
        (ASSETS / "index.html").write_text("<!doctype html><meta charset=utf-8><title>rt_runtime independent host fixture</title>\n")
        with server() as url:
            with urllib.request.urlopen(url + "/index.html") as response:
                if response.headers.get("Cross-Origin-Opener-Policy") or response.headers.get("Cross-Origin-Embedder-Policy"):
                    raise RuntimeError("fixture origin unexpectedly has isolation headers")
            result = browser(url)
        reference = result["reports"][0]
    if not journal.exists():
        raise RuntimeError("actual browser sample/response journal is missing")
    executable = FIXTURE / "target" / args.profile / ("replay.exe" if os.name == "nt" else "replay")
    native = []
    for threaded in (False, True):
        mode = "two-thread" if threaded else "offline"
        output = command([str(executable), str(journal)] + (["--threaded"] if threaded else []), REPORTS / f"native-{mode}.log")
        native.append(json.loads(output.strip().splitlines()[-1]))
    for replay in native:
        for field in ("frames", "sample_hash", "midi_hash", "midi_events", "graph_hash", "max_copy", "max_steps"):
            if replay[field] != reference[field]:
                raise RuntimeError(f"{replay['mode']} differs from real worklet in {field}")
    result["native"] = native
    report_name = f"native-{args.profile}-result.json" if args.native_only else "result.json"
    (REPORTS / report_name).write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    print(f"reports: {REPORTS.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
