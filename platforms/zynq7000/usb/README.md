# ZC702 USB MIDI host

Implementation is in progress. The immutable acceptance criteria and remaining
work are in `PLAN_ZYNQ_USB_MIDI_HOST.md` at the repository root. Physical testing
is deferred until the software is ready for the user's assistance.

## Frozen initial profile

| Resource | Bound |
| --- | ---: |
| Owned controller | USB0, `0xe0002000`, GIC interrupt 53 |
| Non-hub USB devices | 4 |
| External hubs / external tiers | 2 / 2 |
| MIDIStreaming interfaces | 8 |
| Endpoints per device | 8 |
| Logical cable ports per direction | 16 |
| Complete MIDI message / SysEx | 2048 bytes |
| Enumeration descriptor buffer | 4096 bytes |
| Host event queue / events per task iteration | 64 / 16 |
| MIDI endpoint RX/TX FIFO per interface | 1024 bytes each |
| USB task priority / stack reservation | 5 / 32 KiB |
| Control transfer timeout / enumeration attempts | 1000 ms / 2 |
| Shutdown deadline | 5000 ms |

An unsupported or over-budget configuration must produce a refusal. Device,
interface, cable and channel numbers are different resources. Audio remains
dummy audio at 48 kHz and 256 frames; USB ownership is confined to its own task.

TinyUSB is pinned by immutable commit in `pins.json`. It is MIT licensed; retain
its upstream license with redistributed sources and firmware dependency notices.
The platform supplies its own HCD adapter with `OPT_MCU_NONE`, without assuming
another supported MCU's registers or SDK. The local EHCI patch selects the
standard 1024-entry schedule and repeats the 8 ms periodic head throughout it.
This is valid hardware configuration and avoids depending on an emulator's
implementation of the optional short ChipIdea schedules.

## Memory ownership

The linker reserves at most one complete 1 MiB section for TinyUSB DMA storage,
separately from the existing Ethernet DMA reservation. It is mapped as normal
non-cacheable memory before controller initialization. Endpoint buffers are
aligned to 32 bytes and the periodic schedule to 4096 bytes. Transfers retain
their storage until completion. Other buffers use the AMD cache maintenance
hooks and ARM barriers; physical coherency still needs board validation.

The proposed application bridge budget is 2 MiB of normal platform memory for
fixed message queues, per-cable assembly, inventory and recovery state. This is
not an expansion of either allocator arena or the 128 MiB platform ceiling.
Actual link-map sizes, task watermarks and counters must be retained by tests.

## Verification recorded so far

- The warning-denying native workspace build passed on the verified base.
- Pinned QEMU 10.0.13 exposes both ChipIdea controllers. Each capability block
  reports `0x01000040`, one root port and `HCCPARAMS=0x80`.
- An ARM hard-float `build.py --app host --usb` build boots its USB task and full
  FreeRTOS session service on QEMU with no attached device.
- Existing echo, intentional echo-mismatch, production, UI and recovery gates
  passed. The full fixture reached its recording-pool test but failed its
  capacity-boundary assertion; this baseline failure remains to be resolved.

The last two probes are controller/task integration evidence only. MIDI traffic,
hub qualification, board PHY/VBUS, silicon coherency and timing are not yet
qualified. Raw evidence is retained under `work/zynq/` and the task's scratch
directory; source and test changes will add repeatable coverage before delivery.

The USB QEMU build is separate from the watchdog-only deployment emulator:

```sh
python3 platforms/zynq7000/deployment/build_qemu.py --fetch --usb
```

Its model adaptations expose ChipIdea speed status and direct full/low-speed
root connections for functional topology tests. They do not model real TT
microframe scheduling, PHY signaling, VBUS power or DMA cache coherency.
