"""Transport-runner tests use synthetic records and do not launch timing campaigns."""
import copy
import importlib.util
import json
from pathlib import Path
import shlex
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

if sys.platform == "win32":
    raise unittest.SkipTest("Linux measurement runner requires POSIX resource support")

SCRIPTS = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("transport_benchmark", SCRIPTS / "benchmark_remote_dsp_transports.py")
runner = importlib.util.module_from_spec(SPEC)
with mock.patch.object(sys, "path", [str(SCRIPTS), *sys.path]):
    SPEC.loader.exec_module(runner)


class TransportBenchmarkTests(unittest.TestCase):
    def setUp(self):
        self.case = ["screen", [16, 4, 8], "tcp-complete", "other", 0, 0.05]
        self.machine = {"cases": [self.case], "modes": ["other"], "client_cpu": 0, "server_cpu": 2,
                        "arguments": {"sample_rate": 48000, "delay": 0, "stall_us": 2000},
                        "client": {"allowed_cpus": [0, 1], "nice": 0},
                        "server": {"allowed_cpus": [2, 3], "nice": 0}}
        def distribution(count):
            return {"count": count, "histogram_us": [[1, count]], "mean_ns": 1000,
                    "max_ns": 1000, "p50_ns": 1000, "p95_ns": 1000, "p99_ns": 1000, "p999_ns": 1000}
        self.row = {"case": runner.case_name(self.case), "suite": "screen", "variant": "tcp-complete",
                    "mode": "other", "repetition": 0, "status": "measured", "client_exit_code": 0, "server_exit_code": 0,
                    "requested": {"P": 16, "k": 4, "N": 8, "A": 0, "channels": 2, "seconds": 0.05, "warmup": 0},
                    "config": {"packet_frames": 16, "multiplier": 4, "allowance": 8, "channels": 2, "sample_rate": 48000},
                    "measured_packets": 12, "on_time": 12, "late": 0, "missing": 0, "deadline_miss_fraction": 0,
                    "sent": 12, "verified_samples": 384, "unverified_samples": 0, "correctness_errors": 0,
                    "transport": "tcp", "input_policy": "complete", "catch_up": True, "transport_error": None,
                    "source_stall_us": 0, "source_stalled": False,
                    "server": {"concealed_packets": 0, "retired": False},
                    "client_audio": {"nodelay": True}, "server_audio": {"nodelay": True},
                    "client_runtime": {"scheduler": 0, "realtime_priority": 0, "nice": 0, "affinity": "0-1"},
                    "worker_runtime": {"scheduler": 0, "realtime_priority": 0, "nice": 0, "affinity": "2-3"},
                    "rtt": distribution(12), "sender_lateness": distribution(12),
                    "batch_turnaround": distribution(3), "dsp_processing": distribution(3)}

    def test_config_parser_and_cpu_ranges(self):
        self.assertEqual(runner.parse_config("16,4,8"), (16, 4, 8))
        self.assertEqual(runner.cpus("0-2,5,8-9"), [0, 1, 2, 5, 8, 9])
        for value in ["16,4,2", "0,1,1", "16,4", "a,b,c"]:
            with self.subTest(value=value), self.assertRaises(runner.argparse.ArgumentTypeError):
                runner.parse_config(value)

    def test_ssh_command_quotes_paths_and_does_not_accept_option_destinations(self):
        args = SimpleNamespace(server_ssh="user@host", server_root="/a path/project", ssh_port=2222)
        command = runner.server_command(args, ["/a path/bin", "--credential-file", "/a path/key;not-shell"])
        self.assertEqual(command[:-1], ["ssh", "-o", "BatchMode=yes", "-p", "2222", "--", "user@host"])
        self.assertIn(shlex.quote("/a path/project"), command[-1])
        self.assertIn(shlex.quote("/a path/key;not-shell"), command[-1])
        args.server_ssh = "-oProxyCommand=bad"
        with self.assertRaises(ValueError):
            runner.server_command(args, ["binary"])

    def test_cleanup_kills_the_process_group_and_handles_exit_races(self):
        runner.stop_child(None)
        for exited, error in [(False, None), (False, ProcessLookupError()), (True, None)]:
            with self.subTest(exited=exited, error=error):
                child = mock.Mock(pid=12345)
                child.poll.return_value = 0 if exited else None
                with mock.patch.object(runner.os, "killpg", side_effect=error) as kill:
                    runner.stop_child(child)
                if exited:
                    kill.assert_not_called()
                else:
                    kill.assert_called_once_with(12345, runner.signal.SIGKILL)
                child.wait.assert_called_once_with()

    def test_valid_record_and_partial_coverage_are_distinct(self):
        audit = runner.audit_rows([self.row], self.machine)
        self.assertTrue(audit["complete"])
        self.assertEqual(audit["verified_samples"], 384)
        self.assertFalse(runner.audit_rows([], self.machine)["complete"])

    def test_duplicate_and_inconsistent_counts_are_rejected(self):
        with self.assertRaises(AssertionError):
            runner.audit_rows([self.row, self.row], self.machine)
        row = copy.deepcopy(self.row)
        row["missing"] = 1
        with self.assertRaises(AssertionError):
            runner.audit_rows([row], self.machine)

    def test_actual_priority_affinity_and_tcp_nodelay_are_required(self):
        for section, field, bad in [("client_runtime", "scheduler", 1), ("worker_runtime", "affinity", "7"),
                                    ("client_audio", "nodelay", False), ("server_audio", "nodelay", False)]:
            with self.subTest(section=section, field=field):
                row = copy.deepcopy(self.row)
                row[section][field] = bad
                with self.assertRaises(AssertionError):
                    runner.audit_rows([row], self.machine)

    def test_complete_policy_cannot_claim_concealed_input(self):
        row = copy.deepcopy(self.row)
        row["server"]["concealed_packets"] = 1
        with self.assertRaises(AssertionError):
            runner.audit_rows([row], self.machine)

    def test_stall_is_explicit_instead_of_mislabeled_as_normal_operation(self):
        row = copy.deepcopy(self.row)
        row["source_stall_us"] = 2000
        row["source_stalled"] = True
        with self.assertRaises(AssertionError):
            runner.audit_rows([row], self.machine)

    def test_report_reproduces_archive_and_excludes_credentials_from_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "raw"
            output.mkdir()
            (output / "manifest.json").write_text(json.dumps(self.machine))
            (output / f"{self.row['case']}.json").write_text(json.dumps(self.row))
            (output / "credential.key").write_bytes(bytes(32))
            (output / "implementation.patch").write_text("fixture patch\n")
            published = Path(directory) / "published"
            runner.report(output, published)
            before = (output / "results.jsonl.gz").read_bytes()
            runner.report(output)
            self.assertEqual(before, (output / "results.jsonl.gz").read_bytes())
            self.assertEqual(runner.read_rows(published), [self.row])
            self.assertFalse((published / "credential.key").exists())
            self.assertIn("3.0", (published / "summary.csv").read_text())
            self.assertIn("TCP_NODELAY", (published / "audit.json").read_text())


if __name__ == "__main__":
    unittest.main()
