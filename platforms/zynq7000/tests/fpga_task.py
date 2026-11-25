"""Exercise the actual cooperative owner-task lifecycle without FreeRTOS or hardware."""
from pathlib import Path
import os
import subprocess
import tempfile

PLATFORM = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temporary:
    directory = Path(temporary)
    (directory / "FreeRTOS.h").write_text(
        "#include <assert.h>\n#include <stddef.h>\n"
        "#define configASSERT(x) assert(x)\n#define pdPASS 1\n")
    (directory / "task.h").write_text(
        "typedef void *TaskHandle_t;\ntypedef void (*TaskFunction_t)(void *);\n"
        "int xTaskCreate(TaskFunction_t, const char *, unsigned, void *, unsigned, TaskHandle_t *);\n"
        "void vTaskDelete(TaskHandle_t);\nvoid vTaskSuspend(TaskHandle_t);\n"
        "void xTaskNotifyGive(TaskHandle_t);\n")
    output = directory / "fpga-task"
    subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                    "-I" + str(directory), str(PLATFORM / "tests/fpga_task.c"),
                    "-o", str(output)], check=True)
    subprocess.run([output], check=True)
