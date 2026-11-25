"""Exercise the actual direct-load watchdog and every compile-time fault variant."""
from pathlib import Path
import os
import subprocess
import tempfile

PLATFORM = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temporary:
    directory = Path(temporary)
    (directory / "FreeRTOS.h").write_text(
        "#include <assert.h>\n#include <stddef.h>\n"
        "#define configASSERT(x) assert(x)\n#define SHOOP_WATCHDOG_HZ 111111111u\n")
    (directory / "task.h").write_text("void vTaskSuspend(void *);\n")
    (directory / "xil_io.h").write_text(
        "#include <stdint.h>\n#define DATA_SYNC ((void)0)\n"
        "uint32_t Xil_In32(uint32_t);\nvoid Xil_Out32(uint32_t, uint32_t);\n")
    (directory / "xil_printf.h").write_text("int xil_printf(const char *, ...);\n")
    for variant in range(6):
        output = directory / f"watchdog-{variant}"
        subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                        "-DSHOOP_HOST_WATCHDOG", f"-DSHOOP_WATCHDOG_FAULT={variant}",
                        "-I" + str(directory), "-I" + str(PLATFORM / "include"),
                        str(PLATFORM / "tests/watchdog.c"), "-o", str(output)], check=True)
        subprocess.run([output], check=True)
