#!/usr/bin/env python3
"""Check the rt_runtime book and optionally execute its example inventory."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import textwrap
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
PARTS = (
    ("I", "Process audio", 4),
    ("II", "Control a running graph", 4),
    ("III", "Work with stored media", 4),
    ("IV", "Coordinate processors and changes", 4),
    ("V", "Configure and operate the runtime", 3),
)
LINK = re.compile(r"\[[^\]\n]*\]\(([^)\s]+)\)")
FENCE = re.compile(r"^```([^\n]*)\n(.*?)^```\s*$", re.M | re.S)
SOURCE = re.compile(r"<!-- rust-source: ([^\n]+?) -->")


def headings(text):
    """Return GitHub-style heading identifiers, including duplicate suffixes."""
    text = FENCE.sub("", text)
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S)
    counts = {}
    result = set()
    for title in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.M):
        slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        result.add(f"{slug}-{count}" if count else slug)
    return result


def contains_excerpt(source, excerpt):
    """Compare contiguous source lines, allowing only outer indentation changes."""
    expected = textwrap.dedent(excerpt).strip()
    if not expected:
        return False
    lines = source.splitlines()
    length = len(excerpt.strip("\n").splitlines())
    return any(
        textwrap.dedent("\n".join(lines[start : start + length])).strip() == expected
        for start in range(len(lines) - length + 1)
    )


def load_manifest(crate):
    manifest = json.loads((crate / "docs/book.json").read_text())
    pending = manifest["pending_chapters"]
    if not isinstance(pending, list) or any(type(n) is not int or n not in range(1, 20) for n in pending):
        raise ValueError("pending_chapters must list chapter numbers 1–19")
    if len(pending) != len(set(pending)):
        raise ValueError("duplicate pending chapter")
    examples = manifest["examples"]
    names = []
    for entry in examples:
        name = entry["name"]
        if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9_]*", name):
            raise ValueError("invalid example name")
        if not isinstance(entry["args"], list) or any(not isinstance(arg, str) for arg in entry["args"]):
            raise ValueError(f"invalid arguments for {name}")
        names.append(name)
    if not names or len(names) != len(set(names)):
        raise ValueError("example inventory must be nonempty with unique names")
    mappings = manifest["chapter_examples"]
    if not isinstance(mappings, dict):
        raise ValueError("chapter_examples must be an object")
    for number, mapped in mappings.items():
        if number not in {str(n) for n in range(1, 20)}:
            raise ValueError(f"invalid chapter mapping: {number}")
        if not isinstance(mapped, list) or not mapped or any(name not in names for name in mapped):
            raise ValueError(f"invalid example mapping for chapter {number}")
    return manifest


def check_book(crate, allow_pending=False):
    crate = crate.resolve()
    docs = crate / "docs"
    errors = []
    try:
        manifest = load_manifest(crate)
        index = (docs / "README.md").read_text()
    except (OSError, ValueError, KeyError, TypeError) as error:
        return [f"book inventory: {error}"]
    pending = set(manifest["pending_chapters"])
    if pending and not allow_pending:
        errors.append(f"unfinished chapters: {sorted(pending)} (use --allow-pending only during drafting)")
    inventory = {entry["name"] for entry in manifest["examples"]}
    discovered = {path.stem for path in (crate / "examples").glob("*.rs")}
    if discovered != inventory:
        errors.append(f"example inventory differs: missing {sorted(discovered - inventory)}, absent files {sorted(inventory - discovered)}")

    part_headings = re.findall(r"^## Part (\w+) — (.+)$", index, re.M)
    if part_headings != [(roman, title) for roman, title, _ in PARTS]:
        errors.append("contents: expected the five agreed parts in order")
    entries = re.findall(r"^- \[(\d+)\. ([^\]]+)\]\(([^)]+)\)$", index, re.M)
    if [int(number) for number, _, _ in entries] != list(range(1, 20)):
        errors.append("contents: expected chapters 1–19 in order")
        return errors
    sections = re.split(r"^## Part .+$", index, flags=re.M)[1:]
    if [len(re.findall(r"^- \[\d+\. ", section, re.M)) for section in sections] != [part[2] for part in PARTS]:
        errors.append("contents: chapters must be grouped under their agreed parts")
    chapters = []
    for number, title, filename in entries:
        number = int(number)
        path = (docs / filename).resolve()
        if not path.is_relative_to(docs) or not path.name.startswith(f"{number:02d}-"):
            errors.append(f"chapter {number}: invalid numbered path")
            continue
        part = next((i for i, end in enumerate((4, 8, 12, 16, 19), 1) if number <= end), None)
        if not path.parent.name.startswith(f"part-{part}-"):
            errors.append(f"chapter {number}: wrong part directory")
        chapters.append(path)
        if not path.is_file():
            errors.append(f"missing chapter: {filename}")
            continue
        text = path.read_text()
        if number in pending:
            if text.strip():
                errors.append(f"chapter {number}: has content but is still marked pending")
            continue
        if not text.startswith(f"# {number}. {title}\n"):
            errors.append(f"chapter {number}: title differs from contents")
        if len(text.splitlines()) < 10 or re.search(r"\b(TODO|TBD|FIXME)\b", text):
            errors.append(f"chapter {number}: incomplete content")
        if "[Contents](../README.md)" not in text:
            errors.append(f"chapter {number}: missing contents navigation")
        if number != 1 and not manifest["chapter_examples"].get(str(number)):
            errors.append(f"chapter {number}: missing example mapping")
        for direction, neighbor in (("Previous", number - 1), ("Next", number + 1)):
            if 1 <= neighbor <= len(entries):
                target = (docs / entries[neighbor - 1][2]).resolve()
                links = re.findall(rf"\[{direction}:[^\]]+\]\(([^)]+)\)", text)
                if not links or any((path.parent / link).resolve() != target for link in links):
                    errors.append(f"chapter {number}: invalid {direction.lower()} navigation")

    expected = set(chapters)
    actual = set(path.resolve() for path in docs.glob("part-*/*.md"))
    if actual != expected:
        errors.append("chapter files differ from the contents")
    for path in docs.rglob("*.md"):
        text = path.read_text()
        prose = FENCE.sub("", re.sub(r"<!--.*?-->", "", text, flags=re.S))
        for link in LINK.findall(prose):
            parsed = urlsplit(link)
            if parsed.scheme or parsed.netloc:
                continue
            target = (path.parent / unquote(parsed.path)).resolve() if parsed.path else path.resolve()
            if not target.is_file():
                errors.append(f"{path.relative_to(crate)}: missing link {link}")
            elif parsed.fragment and target.suffix == ".md" and unquote(parsed.fragment) not in headings(target.read_text()):
                errors.append(f"{path.relative_to(crate)}: missing anchor {link}")
        markers = list(SOURCE.finditer(text))
        for fence in FENCE.finditer(text):
            language = fence.group(1).strip()
            if language != "rust":
                if language.startswith("rust"):
                    errors.append(f"{path.relative_to(crate)}: Rust excerpts must use plain rust fences")
                continue
            prior = [marker for marker in markers if marker.end() < fence.start()]
            if not prior:
                errors.append(f"{path.relative_to(crate)}: Rust excerpt has no rust-source marker")
                continue
            source = (path.parent / prior[-1].group(1)).resolve()
            if not source.is_relative_to(crate / "examples") or not source.is_file():
                errors.append(f"{path.relative_to(crate)}: excerpt source must be an existing example file")
            elif not contains_excerpt(source.read_text(), fence.group(2)):
                errors.append(f"{path.relative_to(crate)}: Rust excerpt differs from {source.relative_to(crate)}")
    return errors


def run_examples(crate, profile):
    manifest = load_manifest(crate)
    for entry in manifest["examples"]:
        command = ["cargo", "run", "--locked", "-p", "rt_runtime"]
        if profile == "release":
            command.append("--release")
        command += ["--example", entry["name"]]
        if entry["args"]:
            command += ["--", *entry["args"]]
        print("+ " + " ".join(command), flush=True)
        subprocess.run(command, cwd=crate, check=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-pending", action="store_true", help="allow only explicitly listed empty chapters during drafting")
    parser.add_argument("--run-examples", action="store_true", help="execute every inventoried example, including its assertions")
    parser.add_argument("--profile", choices=("debug", "release"), default="debug")
    args = parser.parse_args(argv)
    crate = ROOT / "src/rust/rt_runtime"
    errors = check_book(crate, args.allow_pending)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("rt_runtime book: contents, links, navigation, inventory and excerpts OK", flush=True)
    if args.run_examples:
        try:
            run_examples(crate, args.profile)
        except subprocess.CalledProcessError as error:
            return error.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
