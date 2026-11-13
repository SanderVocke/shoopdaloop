#!/usr/bin/env python3
"""Check runtime documentation links/excerpts and optionally execute examples."""

import argparse
from pathlib import Path
import re
import subprocess
import sys
import textwrap
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
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


def check_book(crate):
    crate = crate.resolve()
    docs = crate / "docs"
    errors = []
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
    for example in sorted((crate / "examples").glob("*.rs")):
        command = ["cargo", "run", "--locked", "-p", "rt_runtime"]
        if profile == "release":
            command.append("--release")
        command += ["--example", example.stem]
        print("+ " + " ".join(command), flush=True)
        subprocess.run(command, cwd=crate, check=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-examples", action="store_true", help="execute every example, including its assertions")
    parser.add_argument("--profile", choices=("debug", "release"), default="debug")
    args = parser.parse_args(argv)
    crate = ROOT / "src/rust/rt_runtime"
    errors = check_book(crate)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("rt_runtime book: links and excerpts OK", flush=True)
    if args.run_examples:
        try:
            run_examples(crate, args.profile)
        except subprocess.CalledProcessError as error:
            return error.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
