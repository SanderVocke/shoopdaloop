# ARM exception journal

The firmware installs explicit undefined-instruction, prefetch-abort and data-abort
handlers after platform initialization. They record diagnostics and halt; they do
not resume the faulting instruction, free uncertain resources or claim that
management remains available after a CPU exception. No new authentication is used.

This is **source/offline-tested, not deployed or physically qualified**. Fatal
recovery still needs a separately qualified hardware watchdog; the direct-load
qualification image does not yet arm one. Failures before handler installation,
corrupted exception stacks and a non-returning hardware/cache access can still
prevent recording. The journal does not make those accesses time-bounded.

## Capture path

1. Mask IRQ, FIQ and asynchronous aborts with CPU instructions, not an RTOS lock.
2. Guard against recursive handler entry.
3. Collect the BSP-saved exception PC, CPSR/SPSR, DFSR/DFAR, IFSR/IFAR, current
   task handle and a 32-bit FNV-1a tag of the build label, when available.
4. Invalidate the record seal, write the body and CRC32, flush caches, then publish
   and flush the seal. No heap, locks, task-name dereference or PL MMIO is used.
5. Attempt polled PS-UART output with a shared **1,000,000-poll** budget, then halt
   with CPU interrupts masked. The handler does not call formatted output or wait
   indefinitely for UART FIFO space. A stalled hardware access remains outside
   that software guarantee.

The record occupies 64 bytes in a reserved 4 KiB page at the end of the platform
memory region. The linker rejects overlap with platform allocations; the builder
checks the symbol address and includes it in the firmware manifest. For Z-turn V2
this is **`0x07fff000`**. It is not in the PL arena or Rust heap. Startup prints
`SHOOP_CRASH_RECORD address=... words=16`.

This page is outside the C startup BSS clear and is cache-flushed on capture.
That does **not** guarantee survival through the factory FSBL, U-Boot, Linux,
power loss or a new ELF load. Read it at an intercepted U-Boot prompt **before**
booting Linux or loading another ELF. Exception-handler initialization explicitly
invalidates and flushes the seal, preventing a previous firmware boot's record
from masquerading as a new capture. No factory boot files or PS/DDR settings are
modified to preserve it.

## Readout

For a matching Z-turn V2 image, the read-only U-Boot command is:

```text
md.l 0x07fff000 10
```

U-Boot counts are hexadecimal: `10` reads sixteen 32-bit words. Raw captures stay
private until audited. The layout is little-endian, version 1:

| Word | Value |
|---:|---|
| 0 | Seal `0x53444352`, published last |
| 1 | Schema `1` |
| 2 | Length in words `16` |
| 3 | Standard CRC32 of words 1–15, excluding word 3, serialized little-endian |
| 4 | Exception kind: undefined 1, prefetch abort 3, data abort 4 |
| 5 | BSP-saved PC |
| 6–7 | CPSR at capture, SPSR of interrupted execution |
| 8–9 | DFSR, DFAR |
| 10–11 | IFSR, IFAR |
| 12 | Current task handle; not a dereferenced task name |
| 13 | Build-label tag; not a cryptographic image identity |
| 14–15 | Reserved zero |

Validate seal/schema/length/CRC before interpreting anything. Decode fault-status
bits before trusting fault addresses. In particular, an imprecise external abort
need not identify the exact instruction/address. Resolve the saved PC using the
**exact credential-bearing ELF kept privately**, not a later rebuild.

The older ABI-13 qualification image had no journal. Its BSP saved three PCs at
`0x0050000c` (undefined), `0x00500010` (prefetch), and `0x00500014` (data abort).
The already-armed inspection reads only those words after physical RESET. They
have no seal/checksum and may be overwritten by the factory boot chain.

## Offline checks

Run in the project development environment:

```sh
python3 platforms/zynq7000/tests/crash.py
```

The actual C handlers are exercised with mocked CPU registers, cache operations,
UART and halt. All three kinds cover normal output, a permanently full FIFO,
recursive entry and interrupted publication. Tests flip every record bit, check
small/zero UART budgets and independently verify CRC32 with Python. The assembly
register capture/masking/halt is compiled for Cortex-A9; this is not an induced
physical CPU-abort or warm-reset-retention qualification.
