"""Synthetic supervisor fixtures: these tests do not execute or replace Miri."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from scripts import run_rt_runtime_miri_jobs as jobs

FIXTURE = r'''
import json,os,signal,subprocess,sys
from pathlib import Path
inventory={"toolchain":"fixture","cases":[["rt_runtime","fixture",[],1,0]],"manifests":{"rt_runtime":"Cargo.toml"},"full_inventory_sha256":"synthetic-inventory","measureme":"--measureme" in sys.argv}
if "--shard" in sys.argv:
 index,total=map(int,sys.argv[sys.argv.index("--shard")+1].split("/"))
 inventory["shard"]={"index":index,"total":total}
if "--list" in sys.argv:
 print(json.dumps(inventory));sys.exit(0)
reports=Path(sys.argv[sys.argv.index("--report-dir")+1]);reports.mkdir()
mode=os.environ.get("MIRI_JOB_FIXTURE","pass")
if mode=="missing":sys.exit(0)
if mode=="sleep":
 with (reports/"fixture.log").open("w") as stream:
  child=subprocess.Popen([sys.executable,"-c","import time;print('partial output',flush=True);time.sleep(60)"],stdout=stream,start_new_session=True)
  def stop(*args):raise InterruptedError("cancel fixture")
  signal.signal(signal.SIGTERM,stop)
  (reports/"case-pid").write_text(str(child.pid))
  try:child.wait()
  except InterruptedError:
   try:os.killpg(child.pid,signal.SIGKILL)
   except ProcessLookupError:pass
   child.wait();sys.exit(143)
 sys.exit(0)
flags="-Zmiri-seed=0"
if inventory["measureme"]:
 (reports/"measureme").mkdir()
 (reports/"measureme/synthetic.mm_profdata").write_text("synthetic profile, not Miri")
 flags+=f" -Zmiri-measureme={reports.resolve() / 'measureme'}"
(reports/"fixture.log").write_text("test result: ok. 1 passed; 0 failed\n")
(reports/"result.json").write_text(json.dumps({"passed":True,"package_filter":None,"toolchain":"fixture","miri":"synthetic fixture, not Miri","manifests":{"rt_runtime":"Cargo.toml"},"shard":inventory.get("shard"),"full_inventory_sha256":inventory["full_inventory_sha256"],"source_commit":os.environ.get("MIRI_SOURCE_COMMIT"),"measureme":inventory["measureme"],"cases":[{"package":"rt_runtime","target":"fixture","arguments":[],"seed":0,"passed":1,"log":"fixture.log","miri_flags":flags}]}))
'''


@unittest.skipUnless(sys.platform == "linux", "Linux process identity/supervision")
class MiriJobTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "Fixture")
        (self.repo / "scripts").mkdir()
        (self.repo / jobs.RUNNER).write_text(FIXTURE)
        (self.repo / "input.txt").write_text("original input")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        (self.repo / ".pi").mkdir()
        (self.repo / ".pi/private").write_text("not part of snapshot")
        (self.repo / "uncommitted.rs").write_text("not in explicit commit")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        shim = self.bin / "nix"
        shim.write_text('#!/bin/sh\nwhile [ "$1" != "--command" ]; do shift; done\nshift\nexec "$@"\n')
        shim.chmod(0o755)
        self.environment = patch.dict(os.environ, {"PATH": str(self.bin) + os.pathsep + os.environ["PATH"]})
        self.environment.start()
        self.running = []

    def git(self, *arguments):
        return subprocess.check_output(["git", *arguments], cwd=self.repo, text=True).strip()

    def tearDown(self):
        for job in self.running:
            process = json.loads((job / "process.json").read_text())
            if jobs.alive(process):
                os.kill(process["pid"], signal.SIGTERM)
                self.wait(job)
            child_path = job / "child.json"
            if child_path.exists() and jobs.alive(json.loads(child_path.read_text())):
                jobs.cancel(job)
                deadline = time.monotonic() + 10
                while jobs.alive(json.loads(child_path.read_text())) and time.monotonic() < deadline:
                    time.sleep(0.02)
            handle = jobs.LAUNCHED.pop(process["pid"], None)
            if handle is not None:
                handle.wait(timeout=10)
        self.environment.stop()
        for path in self.root.rglob("*"):
            if path.is_dir() and not path.is_symlink():
                path.chmod(path.stat().st_mode | 0o700)
        self.temporary.cleanup()

    def prepare(self, mode="pass"):
        job = jobs.prepare_job(self.repo, self.root / "jobs", "fixture-stage", "HEAD", None)
        data = json.loads((job / "job.json").read_text())
        data["environment"]["MIRI_JOB_FIXTURE"] = mode
        jobs.write_json(job / "job.json", data)
        return job

    def launch(self, job):
        jobs.launch(job)
        self.running.append(job)

    def wait(self, job):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            state = jobs.status(job)
            if state["status"] not in ("running", "queued"):
                return state
            time.sleep(0.02)
        self.fail("Synthetic background fixture did not terminate")

    def test_detached_snapshot_survives_development_edits_and_collects(self):
        job = self.prepare()
        original = self.git("rev-parse", "HEAD")
        self.launch(job)
        (self.repo / "input.txt").write_text("changed during background execution")
        self.git("add", "input.txt")
        self.git("commit", "-qm", "later development")
        self.assertEqual((job / "source/input.txt").read_text(), "original input")
        self.assertFalse((job / "source/.pi").exists())
        self.assertFalse((job / "source/uncommitted.rs").exists())
        self.assertEqual(self.wait(job)["status"], "passed")
        result = jobs.collect(job)
        self.assertEqual(result["executions"], 1)
        self.assertEqual(json.loads((job / "collected.json").read_text())["job"]["commit"], original)

    def test_tracked_goal_state_is_excluded_without_losing_standalone_inputs(self):
        inputs = {
            "tests/loop_engine_host/Cargo.toml": "standalone manifest fixture\n",
            "tests/loop_engine_host/Cargo.lock": "standalone lockfile fixture\n",
            "tests/loop_engine_host/src/lib.rs": "standalone source fixture\n",
        }
        for name, content in inputs.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        self.git("add", ".pi/private", "tests/loop_engine_host")
        self.git("commit", "-qm", "tracked state and standalone fixture")
        self.assertEqual(self.git("ls-tree", "--name-only", "HEAD", ".pi"), ".pi")
        job = self.prepare()
        data = json.loads((job / "job.json").read_text())
        self.assertEqual(data["excluded_paths"], [".pi"])
        self.assertEqual(data["environment"]["RUSTFLAGS"], "-D warnings")
        self.assertEqual(data["environment"]["RUSTDOCFLAGS"], "-D warnings")
        self.assertFalse((job / "source/.pi").exists())
        self.assertTrue(all(not name.startswith(".pi/") for name in data["source_manifest"]))
        for name, content in inputs.items():
            self.assertEqual((job / "source" / name).read_text(), content)
            self.assertEqual(data["source_manifest"][name]["sha256"], jobs.digest(self.repo / name))
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "passed")
        self.assertEqual(jobs.collect(job)["executions"], 1)

    def test_manifest_scope_tampering_is_not_passing_evidence(self):
        job = self.prepare()
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "passed")
        path = job / "reports/result.json"
        original = path.read_text()
        report = json.loads(original)
        report["manifests"]["rt_runtime"] = "tests/wrong/Cargo.toml"
        path.write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "Manifest scope mismatch"):
            jobs.collect(job)
        path.write_text(original)
        self.assertEqual(jobs.collect(job)["executions"], 1)

    def test_declared_shard_and_full_inventory_scope_survive_snapshot_and_reject_substitution(self):
        job = jobs.prepare_job(self.repo, self.root / "jobs", "fixture-recovery", "HEAD", None, "1/1")
        data = json.loads((job / "job.json").read_text())
        self.assertEqual(data["shard"], "1/1")
        self.assertEqual(data["command"][-2:], ["--shard", "1/1"])
        self.assertEqual(data["inventory"]["shard"], {"index": 1, "total": 1})
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "passed")
        path = job / "reports/result.json"
        original = path.read_text()
        for changes in [{"shard": {"index": 2, "total": 2}}, {"full_inventory_sha256": "another-selection"},
                        {"source_commit": "another-source"}]:
            path.write_text(json.dumps({**json.loads(original), **changes}))
            with self.subTest(changes=changes), self.assertRaisesRegex(RuntimeError, "scope mismatch|Source commit mismatch"):
                jobs.collect(job)
        path.write_text(original)
        self.assertEqual(jobs.collect(job)["executions"], 1)

    def test_profile_scope_flags_and_artifacts_are_durable_and_cannot_be_substituted(self):
        job = jobs.prepare_job(self.repo, self.root / "jobs", "fixture-profile", "HEAD", None, "1/1", True)
        data = json.loads((job / "job.json").read_text())
        self.assertTrue(data["measureme"])
        self.assertTrue(data["inventory"]["measureme"])
        self.assertEqual(data["command"][-1], "--measureme")
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "passed")
        self.assertEqual(jobs.collect(job)["executions"], 1)
        result = job / "reports/result.json"
        original = result.read_text()
        report = json.loads(original)
        report["measureme"] = False
        result.write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "Selection scope mismatch"):
            jobs.collect(job)
        report = json.loads(original)
        report["cases"][0]["miri_flags"] += " -Zmiri-disable-stacked-borrows"
        result.write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "Interpreter flag scope mismatch"):
            jobs.collect(job)
        result.write_text(original)
        profile = job / "reports/measureme/synthetic.mm_profdata"
        contents = profile.read_bytes()
        profile.unlink()
        with self.assertRaisesRegex(RuntimeError, "Missing diagnostic profile"):
            jobs.collect(job)
        profile.write_bytes(contents + b"changed")
        with self.assertRaisesRegex(RuntimeError, "Artifacts changed"):
            jobs.collect(job)
        profile.write_bytes(contents)
        self.assertEqual(jobs.collect(job)["executions"], 1)

    def test_cli_refuses_a_second_active_job(self):
        job = self.prepare("sleep")
        self.launch(job)
        with patch.object(jobs, "ROOT", self.repo), patch.object(sys, "argv", [
            "jobs", "start", "--stage", "fixture-stage", "--commit", "HEAD",
            "--jobs-dir", str(self.root / "jobs"),
        ]), self.assertRaisesRegex(RuntimeError, "already active"):
            jobs.main()

    def test_zero_exit_without_complete_report_is_not_a_pass(self):
        job = self.prepare("missing")
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "failed")
        with self.assertRaisesRegex(RuntimeError, "Cannot collect"):
            jobs.collect(job)

    def test_result_count_and_log_tampering_are_detected(self):
        job = self.prepare()
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "passed")
        report_path = job / "reports/result.json"
        original = report_path.read_text()
        report = json.loads(original)
        report["cases"][0]["passed"] = 2
        report_path.write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "count/seed mismatch"):
            jobs.collect(job)
        report_path.write_text(original)
        (job / "reports/fixture.log").write_text("test result: ok. 1 passed; 0 failed\nchanged\n")
        with self.assertRaisesRegex(RuntimeError, "Artifacts changed"):
            jobs.collect(job)
        report["cases"] = []
        report_path.write_text(json.dumps(report))
        with self.assertRaisesRegex(RuntimeError, "Incomplete case inventory"):
            jobs.collect(job)

    def test_snapshot_tampering_and_missing_supervisor_do_not_pass(self):
        job = self.prepare()
        self.assertEqual(jobs.status(job)["status"], "interrupted")
        self.launch(job)
        self.assertEqual(self.wait(job)["status"], "passed")
        source = job / "source/input.txt"
        source.chmod(0o644)
        source.write_text("tampered")
        with self.assertRaisesRegex(RuntimeError, "Source snapshot changed"):
            jobs.collect(job)
        self.assertFalse(jobs.alive({"pid": 999999999, "start_ticks": None}))

    def test_lost_supervisor_is_not_success_and_orphan_runner_can_be_cancelled(self):
        job = self.prepare("sleep")
        self.launch(job)
        deadline = time.monotonic() + 10
        while not (job / "reports/case-pid").exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertTrue((job / "reports/case-pid").exists())
        supervisor = json.loads((job / "process.json").read_text())
        os.kill(supervisor["pid"], signal.SIGKILL)
        jobs.LAUNCHED[supervisor["pid"]].wait(timeout=5)
        self.assertEqual(jobs.status(job)["status"], "orphaned")
        with self.assertRaisesRegex(RuntimeError, "Cannot collect"):
            jobs.collect(job)
        self.assertEqual(jobs.cancel(job)["status"], "orphan-cleanup-requested")
        child = json.loads((job / "child.json").read_text())
        deadline = time.monotonic() + 10
        while jobs.alive(child) and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertFalse(jobs.alive(child))
        self.assertIsNone(jobs.identity(int((job / "reports/case-pid").read_text())))
        self.assertEqual(jobs.status(job)["status"], "interrupted")

    def test_cancellation_keeps_partial_output_and_reaps_case_group(self):
        job = self.prepare("sleep")
        self.launch(job)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            log = job / "reports/fixture.log"
            if (job / "reports/case-pid").exists() and log.exists() and "partial output" in log.read_text():
                break
            time.sleep(0.02)
        else:
            self.fail("Fixture did not reach its synthetic case")
        case_pid = int((job / "reports/case-pid").read_text())
        supervisor = json.loads((job / "process.json").read_text())
        os.kill(supervisor["pid"], signal.SIGTERM)
        self.assertEqual(self.wait(job)["status"], "cancelled")
        self.assertIsNone(jobs.identity(case_pid))
        self.assertIn("partial output", log.read_text())
        with self.assertRaisesRegex(RuntimeError, "Cannot collect"):
            jobs.collect(job)


if __name__ == "__main__":
    unittest.main()
