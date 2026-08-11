#!/usr/bin/env python3
"""Run ShoopDaLoop's opt-in traced nextest subset against tracy-collector."""

import argparse
import json
import os
import re
import resource
import shutil
import socket
import struct
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path


def field(value):
    data = value.encode()
    return struct.pack("!H", len(data)) + data


def take_field(data, offset):
    size = struct.unpack_from("!H", data, offset)[0]
    offset += 2
    return data[offset:offset + size].decode(), offset + size


def recv_exact(sock, size):
    result = b""
    while len(result) < size:
        part = sock.recv(size - len(result))
        if not part:
            raise RuntimeError("truncated collector response")
        result += part
    return result


class Collector:
    def __init__(self, endpoint, token):
        host, port = endpoint.rsplit(":", 1)
        self.address = (host, int(port))
        self.token = token

    def request(self, kind, operation=b""):
        payload = field(self.token) + operation
        frame = b"TCOL" + struct.pack("!HHI", 1, kind, len(payload)) + payload
        with socket.create_connection(self.address, timeout=10) as sock:
            sock.settimeout(60)
            sock.sendall(frame)
            header = recv_exact(sock, 12)
            data = recv_exact(sock, struct.unpack("!I", header[8:])[0])
        status = struct.unpack_from("!H", data)[0]
        message, offset = take_field(data, 2)
        if status:
            raise RuntimeError(f"collector request {kind} failed ({status}): {message}")
        return data[offset:]

    def acquire(self, run_id):
        data = self.request(1, field(run_id))
        return take_field(data, 0)[0]

    def decide(self, session, save, source):
        self.request(5, field(session) + struct.pack("!H", 1 if save else 2) + field(source))

    def finalize(self, lease):
        self.request(6, field(lease))


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def free_range(count=16):
    for _ in range(100):
        first = free_port()
        sockets = []
        try:
            for port in range(first, first + count):
                sock = socket.socket()
                sock.bind(("127.0.0.1", port))
                sockets.append(sock)
            return first, first + count - 1
        except OSError:
            pass
        finally:
            for sock in sockets:
                sock.close()
    raise RuntimeError("no free collector port range")


def wait_ready(process, ready):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if ready.exists():
            return json.loads(ready.read_text())
        if process.poll() is not None:
            raise RuntimeError("collector exited before ready")
        time.sleep(0.02)
    raise RuntimeError("collector ready timeout")


def junit_outcomes(path):
    outcomes = {}
    for case in ET.parse(path).getroot().iter("testcase"):
        tags = {child.tag.rsplit("}", 1)[-1] for child in case}
        name = case.attrib.get("name", "")
        outcomes[name] = "failure" if tags & {"failure", "error"} else "success"
    return outcomes


def query_marker(query, trace, marker):
    subprocess.run([str(query), "check", str(trace)], check=True)
    subprocess.run([str(query), "range", str(trace)], check=True, stdout=subprocess.DEVNULL)
    subprocess.run([str(query), "info", str(trace)], check=True, stdout=subprocess.DEVNULL)
    result = subprocess.run(
        [str(query), "query", "--kind", "message", "--filter",
         f"message.text=^{re.escape(marker)}$", "--count", str(trace)],
        check=True, capture_output=True, text=True)
    count = sum(json.loads(line)["count"] for line in result.stdout.splitlines() if line)
    if count < 1:
        raise RuntimeError(f"identity marker missing from {trace}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--collector", type=Path, required=True)
    parser.add_argument("--query", type=Path, required=True)
    parser.add_argument("--nextest", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("artifacts/tracy-nextest"))
    parser.add_argument("--expect-controlled-failures", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    output = (root / args.output).resolve()
    shutil.rmtree(output, ignore_errors=True)
    output.mkdir(parents=True)

    baseline_start = time.monotonic()
    baseline = subprocess.run(
        ["cargo", "test", "--locked", "-p", "shoop_engine", "--test",
         "tracy_collector_contract", "--", "--test-threads=1"], cwd=root)
    baseline_seconds = time.monotonic() - baseline_start
    if baseline.returncode:
        raise RuntimeError("ordinary cargo test lane failed")

    ready = output / "ready.json"
    traces = output / "traces"
    control_port = free_port()
    first, last = free_range()
    daemon_log = open(output / "collector.log", "w", encoding="utf-8")
    daemon = subprocess.Popen(
        [str(args.collector.resolve()), "--output-root", str(traces),
         "--ready-file", str(ready), "--control-port", str(control_port),
         "--data-port-first", str(first), "--data-port-last", str(last),
         "--owner-timeout-ms", "60000", "--connect-timeout-ms", "15000"],
        stdout=daemon_log, stderr=subprocess.STDOUT, text=True)
    nextest_code = 1
    try:
        descriptor = wait_ready(daemon, ready)
        token = Path(descriptor["secret_file"]).read_text().strip()
        client = Collector(descriptor["endpoint"], token)
        lease = client.acquire(descriptor["run_id"])
        env = os.environ.copy()
        env.update({
            "SHOOP_TRACY_NEXTEST": "1",
            "SHOOP_TRACY_DETAIL": "0",
            "TRACY_COLLECTOR_ENDPOINT": descriptor["endpoint"],
            "TRACY_COLLECTOR_TOKEN": token,
            "TRACY_COLLECTOR_RUN_ID": descriptor["run_id"],
            "RUST_BACKTRACE": "0",
        })
        traced_start = time.monotonic()
        nextest = subprocess.run(
            [str(args.nextest.resolve()), "nextest", "run", "--profile",
             "tracy-collector", "--no-fail-fast", "-p", "shoop_engine",
             "--test", "tracy_collector_contract"],
            cwd=root, env=env, capture_output=True, text=True, timeout=180)
        traced_seconds = time.monotonic() - traced_start
        nextest_code = nextest.returncode
        (output / "nextest.stdout.log").write_text(nextest.stdout)
        (output / "nextest.stderr.log").write_text(nextest.stderr)
        if nextest_code not in (100, 101):
            raise RuntimeError(f"unexpected nextest status {nextest_code}")

        junit = root / "target/nextest/tracy-collector/junit.xml"
        if not junit.exists():
            raise RuntimeError("nextest JUnit report missing")
        shutil.copy2(junit, output / "junit.xml")
        outcomes = junit_outcomes(junit)
        # Allow Tracy network threads to observe abrupt exits before decisions.
        time.sleep(0.5)
        manifest = json.loads((traces / "manifest.json").read_text())
        records = manifest["sessions"]
        if len(records) != 4:
            raise RuntimeError(f"expected four registered attempts, got {len(records)}")
        for record in records:
            outcome = outcomes.get(record["test_name"], "unknown")
            client.decide(record["session_id"], outcome != "success", f"junit:{outcome}")
        client.finalize(lease)
        if daemon.wait(timeout=60) != 0:
            raise RuntimeError("collector finalization failed")
        daemon_log.flush()

        manifest = json.loads((traces / "manifest.json").read_text())
        records = manifest["sessions"]
        passing = [r for r in records if r["test_name"].endswith("traced_passes")]
        failures = [r for r in records if not r["test_name"].endswith("traced_passes")]
        if len(passing) != 1 or passing[0]["state"] != "discarded" or passing[0]["output_name"]:
            raise RuntimeError("passing trace was not discarded")
        if len(failures) != 3 or any(r["state"] != "saved" for r in failures):
            raise RuntimeError("failure/abort/timeout traces were not all saved")
        if list(traces.glob("*.partial")):
            raise RuntimeError("partial traces leaked")
        for record in failures:
            marker = (f"shoop-nextest:{record['test_name']}:attempt:{record['retry'] + 1}:"
                      f"id:{record['attempt_id']}")
            query_marker(args.query.resolve(), traces / record["output_name"], marker)

        metrics = {
            "baseline_wall_seconds": baseline_seconds,
            "traced_wall_seconds": traced_seconds,
            "wall_overhead_seconds": traced_seconds - baseline_seconds,
            "peak_rss_kib": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
            "trace_count": len(list(traces.glob("*.tracy"))),
            "trace_bytes": sum(path.stat().st_size for path in traces.glob("*.tracy")),
            "nextest_status": nextest_code,
            "traced_concurrency": 1,
            "detail_tracing": False,
        }
        (output / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")
        print(json.dumps(metrics, indent=2))
    finally:
        if daemon.poll() is None:
            daemon.kill()
            daemon.wait()
        daemon_log.close()

    if not args.expect_controlled_failures:
        raise SystemExit(nextest_code)


if __name__ == "__main__":
    main()
