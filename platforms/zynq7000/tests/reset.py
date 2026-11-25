"""Check the Zynq reset register sequence without executing a physical reset."""
from pathlib import Path
import os
import subprocess
import tempfile

PLATFORM = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temporary:
    directory = Path(temporary)
    (directory / "FreeRTOS.h").write_text(
        "void mock_disable(void);\n#define portDISABLE_INTERRUPTS() mock_disable()\n")
    (directory / "xil_io.h").write_text(
        "#include <stdint.h>\nuint32_t Xil_In32(uintptr_t);\n"
        "void Xil_Out32(uintptr_t, uint32_t);\nvoid mock_barrier(void);\n"
        "#define DATA_SYNC mock_barrier()\n")
    output = directory / "reset"
    subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                    "-I" + str(directory), str(PLATFORM / "reset.c"),
                    str(PLATFORM / "tests/reset.c"), "-o", str(output)], check=True)
    subprocess.run([output], check=True)
