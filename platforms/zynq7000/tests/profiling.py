"""Check boot-scoped task identity reuse and bounded map exhaustion."""
from pathlib import Path
import os
import subprocess
ROOT = Path(__file__).resolve().parents[3]
PLATFORM = ROOT / "platforms/zynq7000"
OUTPUT = ROOT / "work/zynq/profiling-identity-test"
OUTPUT.parent.mkdir(parents=True, exist_ok=True)
subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                "-I" + str(PLATFORM / "include"), str(PLATFORM / "profiling_identity.c"),
                str(PLATFORM / "tests/profiling_identity.c"), "-o", str(OUTPUT)], check=True)
subprocess.run([OUTPUT], check=True)
print("Task identity reuse and capacity: passed")
