"""Native orchestration fixtures do not replace actual Rust or browser execution."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from scripts import run_loop_engine_hosts as runner


class NativeLoopRunnerTests(unittest.TestCase):
    def test_complete_native_profiles_include_all_actors_without_browser_claims(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(
            runner.harness, "command", return_value=json.dumps({"kind": "synthetic-native-report"})
        ) as command, patch.object(runner, "compare_native_graphs") as graphs, patch.object(
            runner.subprocess, "check_output", side_effect=["fixture-source\n", ""]
        ):
            result = runner.native_replays(Path("replay"), Path(directory), "debug")
        self.assertEqual(command.call_count, 17)
        graphs.assert_called_once()
        self.assertEqual(set(result["native"]), {"primitive", "graph", "exports", "staging", "dense", "stalls", "media", "planner"})
        self.assertEqual(result["kind"], "native-replay")
        self.assertEqual(result["source_head"], "fixture-source")
        self.assertFalse(result["source_dirty"])
        self.assertNotIn("reports", result)
        self.assertNotIn("bindingChecks", result)
        options = [call.args[0][1] for call in command.call_args_list]
        self.assertTrue(all(option in options for option in ["--direct", "--wire", "--two-thread", "--planner-json"]))

    def test_mismatched_native_actor_reports_cannot_pass(self):
        def command(arguments, log):
            return json.dumps({"value": 2 if arguments[1] == "--dense-two-thread-json" else 1})
        with tempfile.TemporaryDirectory() as directory, patch.object(
            runner.harness, "command", side_effect=command
        ), patch.object(runner, "compare_native_graphs"), self.assertRaisesRegex(RuntimeError, "dense replay reports differ"):
            runner.native_replays(Path("replay"), Path(directory), "release")

    def test_native_only_cli_never_dispatches_wasm_node_or_browser(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, "ASSETS", Path(directory)), patch.object(
            sys, "argv", ["runner", "--native-only", "--no-build", "--profile", "debug"]
        ), patch.object(runner, "native_replays", return_value={"passed": True, "kind": "native-replay"}) as native, patch.object(
            runner.harness, "command", side_effect=AssertionError("unexpected build/browser command")
        ), patch.object(runner.harness, "browser", side_effect=AssertionError("unexpected browser")), patch.object(
            runner.harness, "ASSETS"
        ), patch.object(runner.harness, "REPORTS"), contextlib.redirect_stdout(io.StringIO()):
            runner.main()
            report = Path(directory) / "reports/native-debug/result.json"
            self.assertEqual(json.loads(report.read_text()), {"passed": True, "kind": "native-replay"})
            native.assert_called_once()

    def test_native_replay_cannot_substitute_for_real_failure_gates(self):
        for flags in (["--verify-failures"], ["--failure-canary", "planner"]):
            with self.subTest(flags=flags), patch.object(sys, "argv", ["runner", "--native-only", *flags]), contextlib.redirect_stderr(
                io.StringIO()
            ), self.assertRaises(SystemExit) as failure:
                runner.main()
            self.assertEqual(failure.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
