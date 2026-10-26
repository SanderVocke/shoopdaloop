"""Runner timeout hygiene; test workloads and the 900-second limit stay intact."""
import os
import signal
import subprocess
import unittest
from unittest.mock import Mock, patch

from scripts import run_rt_runtime_miri as runner


class MiriRunnerTests(unittest.TestCase):
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
