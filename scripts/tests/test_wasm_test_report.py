import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from wasm_test_report import parse_output, write_junit  # noqa: E402


SUCCESS = """running 3 tests
test crate::sync ... ok
test crate::panic ... ok
test crate::later ... ignored, opt-in
test result: ok. 2 passed; 0 failed; 1 ignored; 0 filtered out; finished in 0.01s
"""

FAILURE = """running 1 test
test crate::failure ... FAIL
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 filtered out; finished in 0.01s
"""


class ReportTests(unittest.TestCase):
    def junit(self, output: str, status: int = 0):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "report.xml"
            parsed = write_junit(
                destination,
                package="pilot",
                runtime="node",
                profile="ci",
                command=["wasm-pack", "test"],
                returncode=status,
                elapsed_seconds=1.25,
                output=output,
                extra_properties={
                    "tool.node": "22.22.2",
                    "filters": "[]",
                    "raw_log": "target/wasm-tests/pilot.log",
                },
            )
            return parsed, ET.parse(destination).getroot()

    def test_success_panic_and_ignored_results_are_accounted(self):
        parsed, xml = self.junit(SUCCESS)
        self.assertEqual(parsed.listed, 3)
        self.assertEqual(xml.attrib["tests"], "3")
        self.assertEqual(xml.attrib["failures"], "0")
        self.assertEqual(xml.attrib["skipped"], "1")
        properties = {
            item.attrib["name"]: item.attrib["value"]
            for item in xml.find("properties")
        }
        self.assertEqual(properties["tool.node"], "22.22.2")
        self.assertEqual(properties["filters"], "[]")
        self.assertEqual(properties["raw_log"], "target/wasm-tests/pilot.log")
        self.assertEqual(properties["expected"], "3")
        self.assertEqual(properties["listed"], "3")
        self.assertEqual(properties["executed"], "2")
        self.assertEqual(properties["passed"], "2")
        self.assertEqual(properties["failed"], "0")
        self.assertEqual(properties["ignored"], "1")
        self.assertEqual(
            [case.attrib["name"] for case in xml.findall("testcase")],
            ["pilot::crate::sync", "pilot::crate::panic", "pilot::crate::later"],
        )

    def test_ansi_and_xml_control_bytes_are_sanitized_from_junit(self):
        parsed, xml = self.junit("\x1b[32m" + SUCCESS + "\x1b[0m\x00")
        self.assertEqual(parsed.listed, 3)
        serialized = ET.tostring(xml, encoding="unicode")
        self.assertNotIn("\x1b", serialized)
        self.assertNotIn("\x00", serialized)

    def test_testcase_failure_is_retained(self):
        parsed, xml = self.junit(FAILURE, 1)
        self.assertEqual(parsed.failed, 1)
        self.assertEqual(xml.attrib["failures"], "1")
        self.assertIn("crate::failure", ET.tostring(xml, encoding="unicode"))

    def test_compile_or_browser_crash_gets_synthetic_failure(self):
        _, xml = self.junit("Chrome session crashed before tests\n", 1)
        self.assertGreaterEqual(int(xml.attrib["failures"]), 1)
        self.assertIn("runner_failure", ET.tostring(xml, encoding="unicode"))

    def test_truncated_or_malformed_counts_fail_closed(self):
        parsed, xml = self.junit("running 1 test\ntest crate::lost ... ok\n", 1)
        self.assertTrue(parsed.malformed)
        self.assertGreaterEqual(int(xml.attrib["failures"]), 1)

    def test_count_mismatch_fails_closed(self):
        output = SUCCESS.replace("2 passed", "3 passed")
        parsed, xml = self.junit(output)
        self.assertTrue(parsed.malformed)
        self.assertGreaterEqual(int(xml.attrib["failures"]), 1)

    def test_zero_test_success_fails_closed(self):
        _, xml = self.junit(
            "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.00s\n"
        )
        self.assertGreaterEqual(int(xml.attrib["failures"]), 1)

    def test_timeout_without_summary_fails_closed(self):
        _, xml = self.junit("runner timed out\n", 124)
        text = ET.tostring(xml, encoding="unicode")
        self.assertIn("status 124", text)


if __name__ == "__main__":
    unittest.main()
