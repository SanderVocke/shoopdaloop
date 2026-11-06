import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "check_rt_runtime_docs.py"
SPEC = importlib.util.spec_from_file_location("runtime_docs", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RuntimeDocsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.crate = Path(temporary.name)
        self.docs = self.crate / "docs"
        self.docs.mkdir()
        (self.crate / "examples").mkdir()
        (self.crate / "examples/demo.rs").write_text("fn main() {\n    assert_eq!(2 + 2, 4);\n}\n")
        self.paths = [self.docs / "topic.md"]
        self.index = self.docs / "README.md"
        self.index.write_text("# Runtime guide\n\n[Topic](topic.md)\n")
        self.paths[0].write_text(
            "# Topic\n\n<!-- rust-source: ../examples/demo.rs -->\n\n"
            "```rust\n    assert_eq!(2 + 2, 4);\n```\n"
        )

    def assert_problem(self, fragment):
        errors = MODULE.check_book(self.crate)
        self.assertTrue(any(fragment in error for error in errors), errors)

    def test_complete_book(self):
        self.assertEqual(MODULE.check_book(self.crate), [])

    def test_broken_file_link(self):
        with self.paths[0].open("a") as output:
            output.write("\n[Missing](missing.md)\n")
        self.assert_problem("missing link")

    def test_broken_anchor(self):
        with self.paths[0].open("a") as output:
            output.write("\n[Missing](#absent)\n")
        self.assert_problem("missing anchor")

    def test_heading_ids_ignore_code_and_support_duplicates(self):
        text = "# `ReadIndex` and leases\n## Cut\n## Cut\n```text\n# Not a heading\n```\n"
        self.assertEqual(MODULE.headings(text), {"readindex-and-leases", "cut", "cut-1"})

    def test_changed_excerpt(self):
        self.paths[0].write_text(self.paths[0].read_text().replace("2 + 2, 4", "2 + 2, 5"))
        self.assert_problem("Rust excerpt differs")

    def test_unmarked_excerpt(self):
        self.paths[0].write_text(self.paths[0].read_text().replace("<!-- rust-source: ../examples/demo.rs -->", ""))
        self.assert_problem("has no rust-source")

    def test_nonexample_source(self):
        (self.crate / "fake.rs").write_text("    assert_eq!(2 + 2, 4);\n")
        self.paths[0].write_text(self.paths[0].read_text().replace("../examples/demo.rs", "../fake.rs"))
        self.assert_problem("existing example file")

    def test_excerpt_indentation_is_not_content_drift(self):
        self.assertTrue(MODULE.contains_excerpt("impl X {\n    fn f() {\n        run();\n    }\n}", "fn f() {\n    run();\n}\n"))
        self.assertFalse(MODULE.contains_excerpt("a();\nb();", "a();\nc();"))

    def test_disabled_rust_fences_are_not_a_check_bypass(self):
        path = self.paths[0]
        path.write_text(path.read_text().replace("```rust\n", "```rust,ignore\n"))
        self.assert_problem("plain rust fences")

    def test_runner_discovers_examples_and_executes_both_profiles(self):
        (self.crate / "examples/extra.rs").write_text("fn main() {}\n")
        for profile in ("debug", "release"):
            with self.subTest(profile=profile), patch.object(MODULE.subprocess, "run") as run:
                MODULE.run_examples(self.crate, profile)
                self.assertEqual(
                    [call.args[0][-1] for call in run.call_args_list], ["demo", "extra"]
                )
                for call in run.call_args_list:
                    self.assertEqual("--release" in call.args[0], profile == "release")
                    self.assertTrue(call.kwargs["check"])

    def test_runner_does_not_hide_failure(self):
        with patch.object(MODULE.subprocess, "run", side_effect=subprocess.CalledProcessError(7, "cargo")):
            with self.assertRaises(subprocess.CalledProcessError):
                MODULE.run_examples(self.crate, "debug")


if __name__ == "__main__":
    unittest.main()
