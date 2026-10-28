"""Runner timeout hygiene; test workloads and the 900-second limit stay intact."""
import contextlib
import io
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

from scripts import run_rt_runtime_miri as runner


class MiriRunnerTests(unittest.TestCase):
    def test_inventory_is_available_without_running_miri(self):
        self.assertEqual(sum(c[3] for c in runner.selected_cases()), 298)
        self.assertEqual(sum(c[3] for c in runner.selected_cases("rt_runtime")), 135)
        self.assertEqual(sum(c[3] for c in runner.selected_cases("shoop_loop_engine")), 100)
        for target, count in [("runtime_topology", 28), ("interloop", 2)]:
            cases = [c for c in runner.selected_cases("shoop_loop_engine") if c[1] == target]
            self.assertEqual(len(cases), count)
            self.assertEqual(len({c[2][0] for c in cases}), count)
            self.assertTrue(all(c[2][1:] == ["--", "--exact"] and c[3:] == (1, 0) for c in cases))
        for _, _, arguments, _, _ in runner.selected_cases():
            if "--exact" in arguments:
                self.assertIn("--", arguments)
                self.assertLess(arguments.index("--"), arguments.index("--exact"))

    def test_standalone_inventory_keeps_all_cases_and_concurrency_seeds(self):
        cases = runner.selected_cases(runner.HOST_PACKAGE)
        self.assertEqual(len(cases), 63)
        initial = [case for case in cases if case[4] == 0]
        self.assertEqual(len(initial), 59)
        self.assertEqual(len({case[2][0] for case in initial}), 59)
        self.assertTrue(all(case[1] == "lib" and case[2][1:] == ["--", "--exact"]
                            and case[3] == 1 for case in cases))
        self.assertEqual([(case[2][0], case[4]) for case in cases if case[4]],
                         [(name, seed) for name in runner.HOST_CONCURRENCY for seed in (1, 2)])
        self.assertTrue(set(runner.HOST_CONCURRENCY).issubset(runner.HOST_CASES))
        self.assertEqual(runner.manifest_paths(cases), {runner.HOST_PACKAGE: runner.HOST_MANIFEST})
        arguments = [runner.HOST_CASES[0], "--", "--exact"]
        self.assertEqual(runner.case_command(runner.HOST_PACKAGE, "lib", arguments),
                         ["rustup", "run", runner.TOOLCHAIN, "cargo", "miri", "test", "--locked",
                          "--manifest-path", "tests/loop_engine_host/Cargo.toml", "-p",
                          runner.HOST_PACKAGE, "--lib", *arguments])
        self.assertEqual(runner.case_command("rt_runtime", "capture", []),
                         ["rustup", "run", runner.TOOLCHAIN, "cargo", "miri", "test", "--locked",
                          "-p", "rt_runtime", "--test", "capture"])

    def test_standalone_dispatch_reports_manifest_and_seed_without_interpreting(self):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory)
            selected = [(runner.HOST_PACKAGE, "lib", [runner.HOST_CASES[0], "--", "--exact"], 1, 2)]
            def case(command, env, log):
                self.assertEqual(command, runner.case_command(*selected[0][:3]))
                self.assertEqual(env["MIRIFLAGS"], "-Zmiri-seed=2")
                output = "test result: ok. 1 passed; 0 failed\n"
                log.write_text(output)
                return subprocess.CompletedProcess(command, 0, output)
            with patch.object(sys, "argv", ["runner", "--package", runner.HOST_PACKAGE,
                                           "--report-dir", str(reports)]), patch.object(
                runner, "selected_cases", return_value=selected
            ), patch.object(runner.subprocess, "check_output", return_value="miri fixture"), patch.object(
                runner, "run_case", side_effect=case
            ), patch.object(runner.signal, "signal"), contextlib.redirect_stdout(io.StringIO()):
                runner.main()
            report = json.loads((reports / "result.json").read_text())
            self.assertEqual(report["package_filter"], runner.HOST_PACKAGE)
            self.assertEqual(report["manifests"], {runner.HOST_PACKAGE: runner.HOST_MANIFEST})
            self.assertEqual(report["cases"][0]["seed"], 2)
            self.assertEqual(report["cases"][0]["passed"], 1)

    def test_measureme_is_one_declared_command_not_a_workload_or_flag_override(self):
        with self.assertRaisesRegex(ValueError, "exactly one selected command"):
            runner.inventory(measureme=True)
        selected = runner.inventory(runner.HOST_PACKAGE, (10, 63), True)
        self.assertTrue(selected["measureme"])
        self.assertEqual(selected["cases"], runner.inventory(runner.HOST_PACKAGE, (10, 63))["cases"])
        self.assertEqual(runner.case_flags(2, Path("reports")), "-Zmiri-seed=2")
        with self.assertRaisesRegex(ValueError, "whitespace"):
            runner.case_flags(0, Path("reports with spaces"), True)

    def test_measureme_dispatch_retains_seed_guard_flags_and_durable_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory)
            selected = [(runner.HOST_PACKAGE, "lib", [runner.HOST_CASES[9], "--", "--exact"], 1, 0)]
            expected = f"-Zmiri-seed=0 -Zmiri-measureme={reports / 'measureme'}"
            def case(command, env, log):
                self.assertEqual(command, runner.case_command(*selected[0][:3]))
                self.assertEqual(env["MIRIFLAGS"], expected)
                self.assertEqual(json.loads((reports / "progress.json").read_text())["current"]["miri_flags"], expected)
                output = "test result: ok. 1 passed; 0 failed\n"
                log.write_text(output)
                (reports / "measureme/synthetic.mm_profdata").write_text("synthetic profile, not Miri")
                return subprocess.CompletedProcess(command, 0, output)
            with patch.object(sys, "argv", ["runner", "--measureme", "--report-dir", str(reports)]), patch.object(
                runner, "selected_cases", return_value=selected
            ), patch.object(runner.subprocess, "check_output", return_value="synthetic fixture"), patch.object(
                runner, "run_case", side_effect=case
            ), patch.object(runner.signal, "signal"), patch.dict(os.environ, {"MIRIFLAGS": "-Zmiri-disable-stacked-borrows"}), contextlib.redirect_stdout(io.StringIO()):
                runner.main()
            report = json.loads((reports / "result.json").read_text())
            self.assertTrue(report["measureme"])
            self.assertEqual(report["cases"][0]["miri_flags"], expected)
            self.assertEqual(report["cases"][0]["passed"], 1)

    def test_missing_exact_filter_is_not_a_passing_standalone_result(self):
        with tempfile.TemporaryDirectory() as directory:
            def case(command, env, log):
                output = "test result: ok. 0 passed; 0 failed\n"
                log.write_text(output)
                return subprocess.CompletedProcess(command, 0, output)
            with patch.object(sys, "argv", ["runner", "--report-dir", directory]), patch.object(
                runner, "selected_cases", return_value=[(runner.HOST_PACKAGE, "lib", ["missing", "--", "--exact"], 1, 0)]
            ), patch.object(runner.subprocess, "check_output", return_value="miri fixture"), patch.object(
                runner, "run_case", side_effect=case
            ), patch.object(runner.signal, "signal"), contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(
                RuntimeError, "tests missing"
            ):
                runner.main()
            self.assertFalse((Path(directory) / "result.json").exists())
            self.assertEqual(json.loads((Path(directory) / "progress.json").read_text())["status"], "failed")

    def test_case_output_can_stream_to_a_durable_file(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "case.log"
            result = runner.run_case([sys.executable, "-c", "print('durable output')"], os.environ.copy(), log)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "durable output\n")
            self.assertEqual(log.read_text(), result.stdout)

    def test_external_report_directory_and_progress_without_interpreting_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory)
            def case(command, env, log):
                output = "test result: ok. 1 passed; 0 failed\n"
                log.write_text(output)
                return subprocess.CompletedProcess(command, 0, output)
            with patch.object(sys, "argv", ["runner", "--report-dir", str(reports)]), patch.object(
                runner, "selected_cases", return_value=[("rt_runtime", "fixture", [], 1, 0)]
            ), patch.object(runner.subprocess, "check_output", return_value="miri fixture"), patch.object(
                runner, "run_case", side_effect=case
            ), patch.object(runner.signal, "signal"), contextlib.redirect_stdout(io.StringIO()):
                runner.main()
            report = json.loads((reports / "result.json").read_text())
            progress = json.loads((reports / "progress.json").read_text())
            self.assertTrue(report["passed"])
            self.assertEqual(progress["status"], "passed")
            self.assertEqual(progress["completed"], report["cases"])
            self.assertGreaterEqual(report["cases"][0]["seconds"], 0)

    @unittest.skipUnless(sys.platform == "linux", "POSIX signal/process-group cleanup")
    def test_real_signal_path_reaps_synthetic_case_descendants_and_keeps_partial_log(self):
        from scripts.run_rt_runtime_miri_jobs import identity
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pids = root / "pids.json"
            log = root / "case.log"
            child = root / "child.py"
            child.write_text("import json,os,subprocess,sys,time\n"
                             "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'])\n"
                             f"open({str(pids)!r},'w').write(json.dumps([os.getpid(),child.pid]))\n"
                             "print('partial case output',flush=True)\ntime.sleep(60)\n")
            code = (f"import os,signal,sys;sys.path.insert(0,{str(runner.ROOT)!r});"
                    "from pathlib import Path;from scripts import run_rt_runtime_miri as r;"
                    "signal.signal(signal.SIGTERM,r.interrupted);"
                    f"r.run_case([sys.executable,{str(child)!r}],os.environ.copy(),Path({str(log)!r}))")
            with (root / "parent.log").open("w") as output, subprocess.Popen(
                [sys.executable, "-c", code], stdout=output, stderr=subprocess.STDOUT,
                start_new_session=True,
            ) as parent:
                try:
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        if pids.exists() and log.exists() and "partial case output" in log.read_text():
                            break
                        time.sleep(0.02)
                    else:
                        self.fail("Synthetic case did not start")
                    identifiers = json.loads(pids.read_text())
                    parent.send_signal(signal.SIGTERM)
                    self.assertNotEqual(parent.wait(timeout=10), 0)
                    self.assertTrue(all(identity(pid) is None for pid in identifiers))
                    self.assertIn("partial case output", log.read_text())
                finally:
                    if pids.exists():
                        try:
                            os.killpg(json.loads(pids.read_text())[0], signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    if parent.poll() is None:
                        os.killpg(parent.pid, signal.SIGKILL)
                        parent.wait()

    def test_completed_command_preserves_output_and_status(self):
        child = Mock(pid=123, returncode=7)
        child.communicate.return_value = ("complete output", None)
        with patch.object(runner.subprocess, "Popen") as popen:
            popen.return_value.__enter__.return_value = child
            result = runner.run_case(["miri-command"], {"MIRIFLAGS": "seed"})
        self.assertEqual(result.returncode, 7)
        self.assertEqual(result.stdout, "complete output")
        child.communicate.assert_called_once_with(timeout=900)
        self.assertEqual(popen.call_args.kwargs["start_new_session"], os.name == "posix")
        child.kill.assert_not_called()

    def test_timeout_kills_interpreter_descendants_and_retains_output(self):
        child = Mock(pid=123, returncode=-9)
        child.communicate.side_effect = [
            subprocess.TimeoutExpired(["miri-command"], 900, output="partial"),
            ("complete captured output", None),
        ]
        with patch.object(runner.subprocess, "Popen") as popen, patch.object(
            runner.os, "killpg", create=True
        ) as killpg:
            popen.return_value.__enter__.return_value = child
            with self.assertRaises(subprocess.TimeoutExpired) as failure:
                runner.run_case(["miri-command"], {})
            if os.name == "posix":
                killpg.assert_called_once_with(123, signal.SIGKILL)
                child.kill.assert_not_called()
            else:
                child.kill.assert_called_once_with()
        self.assertEqual(failure.exception.stdout, "complete captured output")
        self.assertEqual(failure.exception.timeout, 900)
        self.assertEqual(child.communicate.call_count, 2)

    def test_timeout_exit_race_still_reaps_and_reports_failure(self):
        if os.name != "posix":
            return
        child = Mock(pid=123, returncode=0)
        child.communicate.side_effect = [subprocess.TimeoutExpired("miri", 900), ("done", None)]
        with patch.object(runner.subprocess, "Popen") as popen, patch.object(
            runner.os, "killpg", side_effect=ProcessLookupError
        ):
            popen.return_value.__enter__.return_value = child
            with self.assertRaises(subprocess.TimeoutExpired) as failure:
                runner.run_case(["miri"], {})
        self.assertEqual(failure.exception.stdout, "done")


if __name__ == "__main__":
    unittest.main()
