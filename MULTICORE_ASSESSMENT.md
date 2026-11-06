# Multicore assessment: using CPU1 on the Zynq-7000 firmware

Status: assessment only, nothing implemented. Single-core optimizations come first.

## Why

On a ZedBoard (Cortex-A9 at 667 MHz, silicon 1.0), the firmware runs everything
on CPU0. CPU1 is held idle. The render task (priority 6) outranks lwIP (4) and
the control task (3) that services app sessions and heartbeats. Measured before
the single-core optimizations:

- One sync loop: render used about 50% of CPU0.
- Sync loop plus one track: about 97%. Control starved, heartbeats expired after
  5 s and the board retired the app session (`shoop_app_connection/src/server.rs:88`).

Since then render has been capped at 80% of each 100 ms window, dropping the
excess as xruns (`platforms/zynq7000/host.c`). Sessions survive overload, but
the UI updates slowly near the cap because control only gets what is left.

Moving render to CPU1 would remove the starvation entirely and give render
about 2x the capacity it has under the cap.

## Options

1. **SMP FreeRTOS on both cores.** Not available here. The pinned kernel is
   FreeRTOS V10.6.1, and AMD's Cortex-A9 port is single-core. Porting SMP
   (cross-core scheduling, spinlocks, IPIs through the GIC) is a large job.
2. **AMP with two images** (a second FreeRTOS or bare-metal image on CPU1, with
   shared-memory IPC). The engine's render and control halves share Rust data
   structures (status cells, queues), so splitting them into separate images
   would need a new IPC layer. Heavy.
3. **Recommended: one image, render as a bare-metal loop on CPU1.** FreeRTOS
   keeps running on CPU0 for lwIP, control and USB. CPU1 runs only the render
   step, without an OS and without interrupts.

## Why option 3 is feasible

- **Render is already allocation-free by contract:** the firmware allocator
  asserts that render never allocates or frees (`firmware/src/lib.rs`), so CPU1
  never touches the FreeRTOS-locked heap.
- **Cross-core state is already safe:** Rust atomics compile to
  `LDREX`/`STREX`/`DMB`, which are correct across cores.
- **Coherent memory is already set up:** the MMU maps DDR shareable and
  write-back, ACTLR.SMP is set, and the SCU keeps both L1 data caches coherent.
  L2 is shared.
- **The profiling and pacing clock is shared:** both cores read the same
  64-bit global timer.

## Work items

1. **Start CPU1 from CPU0** once the engine is prepared. After the BootROM,
   CPU1 waits in a WFE loop for an entry address at `0xFFFFFFF0` plus `SEV`.
   Confirm U-Boot SPL leaves it in the same state for the SD/deployment path.
2. **CPU1 bring-up code:**
   - its own stack
   - the same translation table, with the MMU enabled
   - ACTLR.SMP set, then caches enabled
   - the VFP enabled (CPACR/FPEXC)
   - a minimal vector table for faults.
3. **Pace render on CPU1 from the global timer** (one 256-frame step every
   5.33 ms), with `WFE` between steps or an `SEV` from CPU0's tick. This
   replaces `ulTaskNotifyTake` and the tick-hook phase counter.
4. **Remove FreeRTOS from the render path:**
   - `shoop_in_render()` checks MPIDR (CPU1) instead of the task handle.
   - Start, stop, `render_done` and xrun reporting use plain atomics.
   - The deployment watchdog's render-progress counter is already atomic.
   - The 80% render cap becomes unnecessary.
5. **Audit C code reachable from render:** USB-MIDI (`shoop_usb_*` in
   `render_step`), logging (`SHOOP_RENDER_ERROR` goes through `xil_printf` to
   the shared UART), and any lock that assumes a single core.
6. **JTAG workflow:** the load script must stop holding CPU1 halted, or start
   it itself. `xsdb` can debug both cores.
7. **QEMU and CI:** the runner uses `-smp 1` and asserts one CPU
   (`platforms/zynq7000/qemu.py`). QEMU does not run the BootROM, so the
   secondary start path needs a QEMU-compatible variant: for example, release
   CPU1's reset through the SLCR, or start it with the loader device. Fixture
   (driver-stepped) runs must keep their determinism.

## Risks

- **Coherency bugs:** the `USE_AMP` episode shows how silently cache
  configuration can break DMA. A second core adds more of that surface:
  shareability, the SMP bit, and L2 maintenance by CPU0 while CPU1 renders.
- **Memory contention:** both cores share L2 and DDR bandwidth. Render's
  copy-heavy work could slow CPU0's network handling, and vice versa.
- **Debugging is harder** than with a single FreeRTOS core.

## Effort

A focused piece of work: probably a few days including hardware validation on
the ZedBoard, plus QEMU/CI changes. It doesn't change the engine itself.

## Ordering

Finish the single-core optimizations first: they reduce render cost on every
platform and make the multicore gain easier to measure. Then do the scaling
measurements (empty session, loops, tracks, laptop comparison, quantum size)
before committing to this work.
