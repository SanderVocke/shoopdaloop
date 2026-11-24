"""Exercise PS-XADC sampling without hardware or changes to ADC thermal configuration."""
from pathlib import Path
import os
import subprocess
import tempfile

PLATFORM = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temporary:
    output = Path(temporary) / "temperature"
    subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                    "-I" + str(PLATFORM / "include"), str(PLATFORM / "temperature.c"),
                    str(PLATFORM / "tests/temperature.c"), "-o", str(output)], check=True)
    subprocess.run([output], check=True)
