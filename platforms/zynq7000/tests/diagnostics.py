"""Exercise whole-message diagnostics with a task switch requested at every byte."""
from pathlib import Path
import os
import subprocess
import tempfile

PLATFORM = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temporary:
    directory = Path(temporary)
    (directory / "FreeRTOS.h").write_text("#pragma once\n")
    (directory / "task.h").write_text(
        "#pragma once\nvoid vTaskSuspendAll(void);\nint xTaskResumeAll(void);\n"
    )
    (directory / "xil_printf.h").write_text(
        "#pragma once\nvoid xil_printf(const char *format, ...);\n"
    )
    binary = directory / "diagnostics"
    subprocess.run([
        os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
        "-I" + str(directory), "-I" + str(PLATFORM / "include"),
        str(PLATFORM / "diagnostics.c"), str(PLATFORM / "tests/diagnostics.c"),
        "-o", str(binary),
    ], check=True)
    subprocess.run([binary], check=True)
print("Task diagnostic message serialization: passed")
