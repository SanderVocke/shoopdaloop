#!/usr/bin/env python3
"""Compile and execute the standalone no_std borrowing experiment."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[1]
    probe = root / "docs/design/chunks"
    rustc = shutil.which("rustc")
    if rustc is None:
        raise SystemExit("rustc is required; activate the repository's Rust environment")
    scratch = root / "work/chunk_design"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="chunk-borrow-", dir=scratch) as temporary:
        work = Path(temporary)
        library = work / "libchunk_borrow.rlib"
        base = [rustc, "--edition=2021", "-Dwarnings"]
        subprocess.run(base + ["--crate-name=chunk_borrow", "--crate-type=rlib",
                              str(probe / "borrow_probe.rs"), "-o", str(library)], check=True)
        extern = ["--extern", f"chunk_borrow={library}"]
        executable = work / ("probe.exe" if os.name == "nt" else "probe")
        subprocess.run(base + [str(probe / "borrow_probe_check.rs"), *extern,
                              "-o", str(executable)], check=True)
        subprocess.run([str(executable)], check=True)
        invalid = {
            "escaped_input": ("""use chunk_borrow::AudioMemory;
fn main() {
    let mut memory = AudioMemory::<4>::prepare(2, 2).unwrap();
    let chunk = memory.produce_persistent(|_, output| { output.fill(1.); Ok(()) }).unwrap();
    let mut escaped = None;
    memory.produce_transient(|input, output| {
        escaped = Some(input.get(chunk)?);
        output.fill(2.);
        Ok(())
    }).unwrap();
    assert_eq!(escaped.unwrap()[0], 1.);
}
""", "E0521"),
            "escaped_output": ("""use chunk_borrow::AudioMemory;
fn main() {
    let mut memory = AudioMemory::<4>::prepare(2, 2).unwrap();
    let mut escaped = None;
    memory.produce_transient(|_, output| { escaped = Some(output); Ok(()) }).unwrap();
    escaped.unwrap()[0] = 9.;
}
""", "E0521"),
            "reset_during_borrow": ("""use chunk_borrow::AudioMemory;
fn main() {
    let mut memory = AudioMemory::<4>::prepare(2, 2).unwrap();
    memory.produce_transient(|_, output| {
        memory.finish_quantum()?;
        output.fill(1.);
        Ok(())
    }).unwrap();
}
""", "E0499"),
        }
        for name, (source, expected) in invalid.items():
            path = work / f"{name}.rs"
            path.write_text(source)
            result = subprocess.run(base + [str(path), *extern, "-o", str(work / name)],
                                    text=True, capture_output=True)
            if result.returncode == 0 or expected not in result.stderr:
                raise SystemExit(f"{name}: expected rejection {expected}\n{result.stderr}")
            print(f"Compile-fail check: {name} rejected with {expected}")


if __name__ == "__main__":
    main()
