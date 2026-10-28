#!/usr/bin/env python3
"""Supervised POSIX background Miri jobs from immutable committed source snapshots."""
from __future__ import annotations

import argparse
import datetime
try:
    import fcntl
except ImportError:
    fcntl = None
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
RUNNER = "scripts/run_rt_runtime_miri.py"
LAUNCHED = {}


def stamp():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def manifest(root):
    result = {}
    for path in sorted(root.rglob("*")):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            result[name] = {"symlink": os.readlink(path)}
        elif path.is_file():
            result[name] = {"sha256": digest(path), "executable": bool(path.stat().st_mode & 0o111)}
    return result


def identity(pid):
    """Linux process start ticks prevent signalling a reused PID."""
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        if fields[0] == "Z":
            return None
        boot = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
        return f"{boot}:{fields[19]}"
    except (FileNotFoundError, ProcessLookupError):
        return None


def alive(process):
    return bool(process and process.get("start_ticks") is not None
                and identity(process["pid"]) == process["start_ticks"])


def verify_source(job, data):
    if manifest(job / "source") != data["source_manifest"]:
        raise RuntimeError("Source snapshot changed; result cannot certify the recorded input")
    if digest(job / "supervisor.py") != data["supervisor_sha256"]:
        raise RuntimeError("Supervisor snapshot changed")


def verify_report(job, data):
    verify_source(job, data)
    reports = job / "reports"
    report = json.loads((reports / "result.json").read_text())
    inventory = data["inventory"]
    if (not report.get("passed") or report.get("toolchain") != inventory["toolchain"]
            or not isinstance(report.get("miri"), str) or not report["miri"]):
        raise RuntimeError("Missing pass/version or wrong pinned toolchain")
    if report.get("package_filter") != data["package"]:
        raise RuntimeError("Package scope mismatch")
    if report.get("manifests", {}) != inventory.get("manifests", {}):
        raise RuntimeError("Manifest scope mismatch")
    cases = report.get("cases", [])
    if len(cases) != len(inventory["cases"]):
        raise RuntimeError("Incomplete case inventory")
    artifacts = {"reports/result.json": digest(reports / "result.json")}
    seen = set()
    for case, expected in zip(cases, inventory["cases"]):
        package, target, arguments, count, seed = expected
        if (case.get("package"), case.get("target"), case.get("arguments"),
            case.get("passed"), case.get("seed")) != (package, target, arguments, count, seed):
            raise RuntimeError("Case/filter/count/seed mismatch")
        name = case["log"]
        if Path(name).name != name or name in seen:
            raise RuntimeError("Invalid or duplicate log path")
        seen.add(name)
        path = reports / name
        summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed", path.read_text())
        if not summaries or sum(int(n) for n, _ in summaries) != count or any(int(n) for _, n in summaries):
            raise RuntimeError("Missing or mismatched passing test summaries")
        artifacts[f"reports/{name}"] = digest(path)
    return {"executions": sum(c[3] for c in inventory["cases"]), "artifact_sha256": artifacts}


def prepare_job(repo, jobs, stage, commit, package):
    repo = repo.resolve()
    sha = subprocess.check_output(["git", "rev-parse", "--verify", f"{commit}^{{commit}}"], cwd=repo, text=True).strip()
    entries = subprocess.check_output(["git", "ls-tree", "-r", sha], cwd=repo, text=True).splitlines()
    if any(line.startswith("160000 ") for line in entries):
        raise RuntimeError("Submodules need an explicit dependency snapshot; refusing an incomplete export")
    paths = subprocess.check_output(["git", "ls-tree", "-z", "--name-only", sha], cwd=repo).decode().split("\0")
    paths = [f":(literal){name}" for name in paths if name and name != ".pi"]
    job = jobs / (time.strftime("%Y%m%dT%H%M%S", time.gmtime()) + "-" + uuid.uuid4().hex[:10])
    job.mkdir(parents=True)
    source = job / "source"
    source.mkdir()
    archive = job / "source.tar"
    subprocess.run(["git", "archive", "--format=tar", "--output", str(archive), sha, "--", *paths], cwd=repo, check=True)
    archive_hash = digest(archive)
    with tarfile.open(archive) as stream:
        stream.extractall(source, filter="data")
    archive.unlink()
    command = [sys.executable, RUNNER, "--list"]
    if package:
        command += ["--package", package]
    inventory = json.loads(subprocess.check_output(command, cwd=source, text=True))
    if not inventory.get("cases") or not inventory.get("toolchain"):
        raise RuntimeError("Snapshot runner supplied no inventory")
    source_manifest = manifest(source)
    for path in sorted(source.rglob("*"), reverse=True):
        if not path.is_symlink():
            path.chmod(path.stat().st_mode & ~0o222)
    source.chmod(source.stat().st_mode & ~0o222)
    shutil.copy2(Path(__file__), job / "supervisor.py")
    command = ["nix", "develop", "--no-update-lock-file", f"path:{source}", "--command", "python3", RUNNER, "--report-dir", str(job / "reports")]
    if package:
        command += ["--package", package]
    data = {"id": job.name, "stage": stage, "commit": sha, "package": package,
            "excluded_paths": [".pi"],
            "created_at": stamp(), "status": "queued", "inventory": inventory,
            "source_manifest": source_manifest, "source_archive_sha256": archive_hash,
            "supervisor_sha256": digest(job / "supervisor.py"), "command": command,
            "environment": {"CARGO_TARGET_DIR": str(job / "build"),
                            "GIT_CEILING_DIRECTORIES": str(job),
                            "RUSTFLAGS": "-D warnings", "RUSTDOCFLAGS": "-D warnings",
                            "MIRI_SOURCE_COMMIT": sha,
                            "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".cache/rt-runtime-miri-rustup"))}}
    write_json(job / "job.json", data)
    return job


def launch(job):
    with (job / "supervisor.log").open("w") as output:
        process = subprocess.Popen([sys.executable, str(job / "supervisor.py"), "_run", str(job)],
                                   stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT,
                                   start_new_session=True, close_fds=True)
    for pid, previous in list(LAUNCHED.items()):
        if previous.poll() is not None:
            del LAUNCHED[pid]
    LAUNCHED[process.pid] = process
    token = identity(process.pid)
    write_json(job / "process.json", {"pid": process.pid, "start_ticks": token})
    return process.pid


def supervise(job):
    data = json.loads((job / "job.json").read_text())
    started = time.monotonic()
    state = {"status": "running", "started_at": stamp(), "pid": os.getpid()}
    write_json(job / "state.json", state)
    child = None
    cancelled = False

    def stop(_signum, _frame):
        nonlocal cancelled
        cancelled = True
        raise InterruptedError("Background job cancellation requested")

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        verify_source(job, data)
        with (job / "runner.log").open("w") as output:
            child = subprocess.Popen(data["command"], cwd=job / "source",
                                     env={**os.environ, **data["environment"]},
                                     stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT,
                                     start_new_session=True, close_fds=True)
            write_json(job / "child.json", {"pid": child.pid, "start_ticks": identity(child.pid)})
            code = child.wait()
        state["exit_code"] = code
        if code:
            raise RuntimeError(f"Runner exited {code}; retain runner/per-case output")
        state.update(verify_report(job, data))
        state["status"] = "passed"
    except BaseException as error:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                child.wait(timeout=15)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
        state.update(status="cancelled" if cancelled else "failed", error=str(error))
    finally:
        state.update(ended_at=stamp(), seconds=time.monotonic() - started)
        write_json(job / "state.json", state)


def status(job):
    data = json.loads((job / "job.json").read_text())
    state = json.loads((job / "state.json").read_text()) if (job / "state.json").exists() else {"status": "queued"}
    process = json.loads((job / "process.json").read_text()) if (job / "process.json").exists() else None
    if state["status"] in ("queued", "running") and not alive(process):
        child = json.loads((job / "child.json").read_text()) if (job / "child.json").exists() else None
        state = {**state, "status": "orphaned" if alive(child) else "interrupted",
                 "error": "No live supervisor and no terminal result; inspect/cancel any recorded live runner; not passing evidence"}
    return {"id": data["id"], "stage": data["stage"], "commit": data["commit"], **state}


def collect(job):
    state = status(job)
    if state["status"] != "passed" or state.get("exit_code") != 0:
        raise RuntimeError(f"Cannot collect passing evidence: {state['status']}")
    data = json.loads((job / "job.json").read_text())
    checked = verify_report(job, data)
    if checked["artifact_sha256"] != state["artifact_sha256"] or checked["executions"] != state["executions"]:
        raise RuntimeError("Artifacts changed after supervisor verification")
    evidence = {"job": data, "result": state, "collected_at": stamp(),
                "limitations": ["Proves only the recorded committed snapshot and inventory, not later edits or all workspace tests.",
                                "A snapshot pass does not imply final-head CI or review approval."]}
    write_json(job / "collected.json", evidence)
    return {"id": data["id"], "status": "passed", "executions": checked["executions"], "evidence": str(job / "collected.json")}


def cancel(job):
    process = json.loads((job / "process.json").read_text())
    if alive(process):
        os.kill(process["pid"], signal.SIGTERM)
        return {"job": str(job), "status": "cancellation-requested"}
    child = json.loads((job / "child.json").read_text()) if (job / "child.json").exists() else None
    if alive(child):
        os.killpg(child["pid"], signal.SIGTERM)
        return {"job": str(job), "status": "orphan-cleanup-requested"}
    raise RuntimeError("No known live supervisor/runner; retain interrupted artifacts")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    start = commands.add_parser("start")
    start.add_argument("--stage", required=True)
    start.add_argument("--commit", required=True, help="Explicit committed snapshot; uncommitted edits are never included")
    start.add_argument("--package", choices=("rt_runtime", "shoop_loop_engine", "loop_engine_host_fixture"))
    start.add_argument("--jobs-dir", type=Path, default=ROOT / "target/loop-layer-miri-jobs")
    for name in ("status", "collect", "cancel", "_run"):
        commands.add_parser(name).add_argument("job", type=Path)
    args = parser.parse_args()
    if fcntl is None or os.name != "posix" or not Path("/proc/self/stat").exists():
        parser.error("Background supervisor currently requires Linux /proc identities; foreground runner remains portable")
    if args.action == "start":
        jobs = args.jobs_dir.resolve()
        jobs.mkdir(parents=True, exist_ok=True)
        with (jobs / ".launch.lock").open("w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for existing in jobs.iterdir():
                if existing.is_dir() and (existing / "job.json").exists():
                    if status(existing)["status"] in ("running", "queued", "orphaned"):
                        raise RuntimeError(f"Miri job already active: {existing}")
            job = prepare_job(ROOT, jobs, args.stage, args.commit, args.package)
            pid = launch(job)
        print(json.dumps({"job": str(job), "pid": pid, "commit": json.loads((job / "job.json").read_text())["commit"], "status": "launched", "note": "Not a passing result; continue development and collect later."}))
    elif args.action == "_run":
        supervise(args.job.resolve())
    elif args.action == "status":
        print(json.dumps(status(args.job.resolve()), indent=2))
    elif args.action == "collect":
        print(json.dumps(collect(args.job.resolve()), indent=2))
    else:
        print(json.dumps(cancel(args.job.resolve())))


if __name__ == "__main__":
    main()
