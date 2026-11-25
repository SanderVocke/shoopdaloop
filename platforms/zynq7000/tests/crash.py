"""Exercise the actual exception handlers and crash journal without ARM faults."""
from pathlib import Path
import os
import subprocess
import struct
import tempfile
import zlib

PLATFORM = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as temporary:
    directory = Path(temporary)
    headers = {
        "FreeRTOS.h": "void mock_disable(void);\n#define portDISABLE_INTERRUPTS() mock_disable()\n",
        "task.h": "void *xTaskGetCurrentTaskHandle(void);\n",
        "xil_cache.h": "#include <stdint.h>\ntypedef intptr_t INTPTR;\ntypedef uint32_t u32;\nvoid Xil_DCacheFlushRange(intptr_t, uint32_t);\n",
        "xil_exception.h": "#pragma once\n#include <stdint.h>\n#define XIL_EXCEPTION_ID_UNDEFINED_INT 1u\n#define XIL_EXCEPTION_ID_PREFETCH_ABORT_INT 3u\n#define XIL_EXCEPTION_ID_DATA_ABORT_INT 4u\nvoid Xil_ExceptionRegisterHandler(uint32_t, void (*)(void *), void *);\n",
        "xil_io.h": "void mock_barrier(void);\n#define DATA_SYNC mock_barrier()\n",
        "xuartps.h": "#include <stdint.h>\n#define STDOUT_BASEADDRESS 0xe0001000u\n#define XUARTPS_SR_OFFSET 0x2cu\n#define XUARTPS_FIFO_OFFSET 0x30u\n#define XUARTPS_SR_TXFULL 16u\nuint32_t mock_uart_read(uintptr_t, unsigned);\nvoid mock_uart_write(uintptr_t, unsigned, uint32_t);\n#define XUartPs_ReadReg mock_uart_read\n#define XUartPs_WriteReg mock_uart_write\n",
        "xil_printf.h": "int xil_printf(const char *, ...);\n",
    }
    for name, content in headers.items():
        (directory / name).write_text(content)
    output = directory / "crash"
    subprocess.run([os.environ.get("CC", "cc"), "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                    "-I" + str(directory), "-I" + str(PLATFORM / "include"),
                    str(PLATFORM / "crash.c"), str(PLATFORM / "exceptions.c"),
                    str(PLATFORM / "tests/crash.c"), "-o", str(output)], check=True)
    for kind in (1, 3, 4):
        for mode in ("normal", "blocked", "nested", "torn"):
            text = subprocess.check_output([output, str(kind), mode], text=True)
            if mode != "torn":
                line = next(line for line in text.splitlines() if line.startswith("MOCK_CRASH_WORDS"))
                words = [int(word, 16) for word in line.split()[1:]]
                assert len(words) == 16 and words[:3] == [0x53444352, 1, 16]
                body = b"".join(struct.pack("<I", word) for i, word in enumerate(words) if i not in (0, 3))
                assert words[3] == zlib.crc32(body), "C journal checksum differs from standard CRC32"
            print(text.splitlines()[-1])
