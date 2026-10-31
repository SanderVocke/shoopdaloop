import importlib.util
import json
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
        self.manifest = {
            "pending_chapters": [],
            "examples": [{"name": "demo", "args": []}],
            "chapter_examples": {str(n): ["demo"] for n in range(2, 20)},
        }
        self.paths = []
        index = ["# Book", ""]
        number = 1
        for part, (roman, title, count) in enumerate(MODULE.PARTS, 1):
            index += [f"## Part {roman} — {title}", ""]
            directory = self.docs / f"part-{part}-topics"
            directory.mkdir()
            for _ in range(count):
                path = directory / f"{number:02d}-topic.md"
                self.paths.append(path)
                index.append(f"- [{number}. Topic {number}]({path.relative_to(self.docs)})")
                number += 1
            index.append("")
        self.index = self.docs / "README.md"
        self.index.write_text("\n".join(index))
        for number, path in enumerate(self.paths, 1):
            navigation = ["[Contents](../README.md)"]
            if number > 1:
                navigation.append(f"[Previous: Topic {number - 1}](../{self.paths[number - 2].relative_to(self.docs)})")
            if number < 19:
                navigation.append(f"[Next: Topic {number + 1}](../{self.paths[number].relative_to(self.docs)})")
            path.write_text(
                f"# {number}. Topic {number}\n\n"
                + " · ".join(navigation)
                + "\n\n<!-- rust-source: ../../examples/demo.rs -->\n\n## Calculation\n\n"
                + "A checked result.\n\n```rust\n    assert_eq!(2 + 2, 4);\n```\n"
            )
        self.save_manifest()

    def save_manifest(self):
        (self.docs / "book.json").write_text(json.dumps(self.manifest))

    def assert_problem(self, fragment, allow_pending=False):
        errors = MODULE.check_book(self.crate, allow_pending)
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
        self.paths[0].write_text(self.paths[0].read_text().replace("<!-- rust-source: ../../examples/demo.rs -->", ""))
        self.assert_problem("has no rust-source")

    def test_nonexample_source(self):
        (self.crate / "fake.rs").write_text("    assert_eq!(2 + 2, 4);\n")
        self.paths[0].write_text(self.paths[0].read_text().replace("../../examples/demo.rs", "../../fake.rs"))
        self.assert_problem("existing example file")

    def test_excerpt_indentation_is_not_content_drift(self):
        self.assertTrue(MODULE.contains_excerpt("impl X {\n    fn f() {\n        run();\n    }\n}", "fn f() {\n    run();\n}\n"))
        self.assertFalse(MODULE.contains_excerpt("a();\nb();", "a();\nc();"))

    def test_empty_chapter_requires_explicit_drafting_allowance(self):
        self.paths[1].write_text("")
        self.manifest["pending_chapters"] = [2]
        self.save_manifest()
        self.assert_problem("unfinished chapters")
        self.assertEqual(MODULE.check_book(self.crate, allow_pending=True), [])

    def test_pending_list_must_shrink_when_content_arrives(self):
        self.manifest["pending_chapters"] = [2]
        self.save_manifest()
        self.assert_problem("still marked pending", allow_pending=True)

    def test_missing_example_inventory_entry(self):
        (self.crate / "examples/extra.rs").write_text("fn main() {}\n")
        self.assert_problem("example inventory differs")

    def test_missing_chapter_mapping(self):
        del self.manifest["chapter_examples"]["2"]
        self.save_manifest()
        self.assert_problem("missing example mapping")

    def test_bad_chapter_order(self):
        self.index.write_text(self.index.read_text().replace("- [2. Topic 2]", "- [3. Topic 2]"))
        self.assert_problem("chapters 1–19 in order")

    def test_bad_part_order(self):
        self.index.write_text(self.index.read_text().replace("Part I — Process audio", "Part V — Process audio"))
        self.assert_problem("five agreed parts")

    def test_wrong_part_grouping(self):
        text = self.index.read_text()
        heading = "## Part II — Control a running graph\n\n"
        text = text.replace(heading, "")
        text = text.replace("- [4. Topic 4]", heading + "- [4. Topic 4]")
        self.index.write_text(text)
        self.assert_problem("grouped under their agreed parts")

    def test_unlisted_chapter_file(self):
        (self.paths[0].parent / "extra.md").write_text("# Extra\n")
        self.assert_problem("chapter files differ")

    def test_disabled_rust_fences_are_not_a_check_bypass(self):
        path = self.paths[0]
        path.write_text(path.read_text().replace("```rust\n", "```rust,ignore\n"))
        self.assert_problem("plain rust fences")

    def test_wrong_navigation(self):
        path = self.paths[1]
        path.write_text(path.read_text().replace("[Next: Topic 3]", "[Previous: Topic 3]"))
        self.assert_problem("navigation")

    def test_missing_chapter_file(self):
        self.paths[1].unlink()
        self.assert_problem("missing chapter")

    def test_missing_inventory_file(self):
        (self.docs / "book.json").unlink()
        self.assert_problem("book inventory")

    def test_duplicate_example_inventory_entry(self):
        self.manifest["examples"].append({"name": "demo", "args": []})
        self.save_manifest()
        self.assert_problem("unique names")

    def test_runner_executes_both_profiles_with_arguments(self):
        self.manifest["examples"][0]["args"] = ["--iterations", "1024"]
        self.save_manifest()
        for profile in ("debug", "release"):
            with self.subTest(profile=profile), patch.object(MODULE.subprocess, "run") as run:
                MODULE.run_examples(self.crate, profile)
                command = run.call_args.args[0]
                self.assertEqual("--release" in command, profile == "release")
                self.assertEqual(command[-3:], ["--", "--iterations", "1024"])
                self.assertTrue(run.call_args.kwargs["check"])

    def test_runner_does_not_hide_failure(self):
        with patch.object(MODULE.subprocess, "run", side_effect=subprocess.CalledProcessError(7, "cargo")):
            with self.assertRaises(subprocess.CalledProcessError):
                MODULE.run_examples(self.crate, "debug")


if __name__ == "__main__":
    unittest.main()
