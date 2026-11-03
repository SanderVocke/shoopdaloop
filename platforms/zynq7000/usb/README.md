# ZC702 USB MIDI host

The implementation is opt-in. The immutable acceptance criteria and remaining
work are in `PLAN_ZYNQ_USB_MIDI_HOST.md` at the repository root. Physical ZC702
qualification is pending; use [the qualification checklist](QUALIFICATION.md)
when the automated and PR checks are complete.

## Frozen initial profile

| Resource | Bound |
| --- | ---: |
| Owned controller | USB0, `0xe0002000`, GIC interrupt 53 |
| Non-hub USB devices | 4 |
| External hubs / external tiers | 2 / 2 |
| MIDIStreaming interfaces | 8 |
| Endpoint numbers per device | 1–7 in each direction, plus EP0 |
| Interface numbers per device | 0–31 |
| Downstream ports per hub | 1–31 |
| Logical cable ports per direction | 16 |
| Complete MIDI message / SysEx | 2048 bytes |
| Enumeration descriptor buffer | 4096 bytes |
| Host event queue / events per task iteration | 64 / 16 |
| MIDI endpoint RX/TX FIFO per interface | 1024 bytes each |
| Complete-message queue per logical port | 256 entries |
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

The application bridge budget is 32 MiB of normal platform memory for
fixed message queues, per-cable assembly, inventory and recovery state. This is
not an expansion of either allocator arena or the 128 MiB platform ceiling.
Actual link-map sizes, task watermarks and counters must be retained by tests.

The queue includes room for the engine's normal 32-message reset burst. A
16-entry prototype overflowed during exact SysEx playback. Measured playback
includes reset and state-restoration bursts as well as recorded messages; a
64-entry queue also overflowed when SysEx remained in progress across a loop
boundary. The queue admits a complete 256-event render block. Overload still
triggers reserved recovery rather than allocating more memory.

## Build and connect

Use the ARM GCC/newlib and Rust versions pinned by the platform's build/CI
instructions. Fetch pristine sources, build the isolated USB emulator, then build
the firmware using a private 32-byte credential file:

```sh
python3 platforms/zynq7000/deployment/build_qemu.py --fetch --usb
python3 platforms/zynq7000/build.py --fetch --app host --usb --credential-file /private/host.key
```

Outputs are `work/zynq/build/host-usb.{elf,map,json}` and
`host-usb.licenses.txt`. Direct-load ELF files embed the supplied credential and
must be kept private; share the generic deployment firmware and public evidence.
The manifest records source pins, local patch hashes,
source-input hash, tool versions and ELF hash. `--deployment --usb` selects the
existing signed SD/network deployment path and produces `host-usb-deployment.elf`.
Omitting `--usb` retains the original dummy-MIDI fixtures. Do not run another USB
controller driver alongside TinyUSB.

For hardware, use ZC702 USB0's ULPI connector and the board revision's documented
host/OTG adapter and power settings. The platform configures MIO28–39, resets the
PHY using MIO7, verifies ULPI identity/scratch access and enables VBUS drive.
The PS DDR/clock initialization must match the ZC702 reference profile. Start
with a powered hub or self-powered instrument; qualify bus power separately.
A USB-to-DIN converter needs no additional driver. Its firmware owns DIN
serialization. USB audio and proprietary USB interfaces are unsupported.

Connect the native UI using the existing authenticated remote-host profile.
Select ports in the connection dialog. Each virtual cable is one selectable
logical port carrying sixteen MIDI channels. Product labels include topology,
MIDIStreaming interface and cable. The board MIO/reset values follow the
[pinned ZC702 PS initialization](https://github.com/u-boot/u-boot/blob/5508406582f6f7120dce0c4b3059819a41788db2/board/xilinx/zynq/zynq-zc702/ps7_init_gpl.c). Offline ports with saved routes remain visible
as disconnected and can be disconnected explicitly. New connections require an
available port. Saved missing-device routes reconnect when that identity returns.
Firmware and clients must use session protocol version 12 together.

## Identity and admission

IDs combine VID/PID, a hash of the serial when available, physical hub topology,
interface, direction and cable. Topology is included even with a serial: devices
with duplicate serials remain distinct without changing an existing route's ID.
Moving a device to another physical hub port changes its identity and requires
reselection. USB address and TinyUSB class index never identify a saved route.

Bindings stay pinned while a confirmed, desired or pending route references them,
including routes in a session replacement. Control retires an offline identity
before USB may reclaim its slot; old snapshots cannot resurrect that binding.
Disconnected, unreferenced bindings can be reclaimed. Offline routed bindings
reserve logical-port capacity; disconnect unused routes and reattach a device
that was refused for capacity. A generation invalidates
queued traffic on disconnect/reuse. Device/interface/cable admission is bounded;
refusal logs identify descriptor, topology or port resource failures. Unsupported
classes are ignored; supported MIDI functions in composite audio devices remain
available. The supported shape is default alternate setting 0, USB-MIDI 1.0
MIDIStreaming, one bulk endpoint per supported direction and 1–16 cable
associations. MIDI 2.0/UMP alternate settings are ignored. Configurations over
4096 bytes, endpoint numbers above 7 and hub port counts above 31 are refused.

## Scheduling and recovery

TinyUSB and its callbacks run only in the USB task. Render input reads at most
64 complete events per port per block into prepared 4096-byte buffers; individual
messages remain limited to 2048 bytes. Input arrival uses the monotonic platform
clock. Frame offsets describe the preceding render interval, clamp old/future
arrivals to the block, and preserve queue ordering. Output deadlines round frame
offsets upward to microseconds; late output sends on the next USB iteration.
The USB task processes at most 16 host events, 128 incoming packets per interface
and 128 successful outgoing packets per iteration, rotating across output ports.
These are transport timestamps, not a physical sample-accurate delivery promise.

Per-cable SysEx assembly expires after one second. Malformed packets, oversized
or incomplete messages, pressure and disconnect discard bounded state and request
recovery. Realtime messages emit immediately; a complete SysEx emits at its end,
so engine event order follows message completion. Playback packetizes each
complete SysEx with realtime represented as separate engine events. Reserved recovery sends
All Sound Off and All Notes Off on all sixteen channels (32 messages); ordinary
output waits for recovery completion. Pressure counters distinguish accepted,
completed, refused, dropped, malformed and recovery events. Messages superseded by recovery
or invalidated on disconnect count as dropped; pending USB packet loss is reported
separately. Final counters include queued messages and completed packet counts.

Shutdown stops ingress, waits for engine recovery and drains live-device USB
transfers for at most five seconds of guest/platform time. Queue acceptance does
not count as USB completion. Lost devices report lost traffic; stalled endpoints
become unavailable. An unresponsive output produces a bounded shutdown refusal,
with control/management still live, and the controller is stopped before buffers
can be released. USB completion does not prove that a synth played a note or that
a converter finished its DIN output. Serial logs include controller/PHY failures,
mount/refusal events, pressure and final drain/error/stack diagnostics.

## Repeatable tests and evidence

```sh
python3 platforms/zynq7000/usb/tests/run.py
python3 platforms/zynq7000/usb/suite.py --fetch
```

The first command needs fetched pristine TinyUSB sources and GCC with ASan/UBSan.
For the EHCI queue-head test, GCC multilib and its 32-bit sanitizer libraries
are also required (`gcc-multilib` on the pinned Debian CI image). On hosts without
32-bit process support, that case uses `qemu-i386` from `qemu-user`; this runs the
queue-head unit, independently of the system/USB emulator. MIDI/bridge/fake-HCD
tests execute natively. It tests
independent MIDI golden vectors, descriptor bounds, fixed queues,
generations, clock edges and reserved recovery. A fake HCD runs the actual pinned
TinyUSB core/MIDI/hub drivers, including eight interfaces across four devices,
one-direction/composite ports, malformed/short enumeration replies, endpoint
stalls, control timeout, device capacity, address reuse, nested FS/STT/MTT hub
descriptors and change bits for hub ports 8, 15 and 31. Descriptor cases do not
prove real transaction-translator scheduling. A separate test compiles the actual
patched EHCI queue-head constructor for 32-bit execution and checks nearest
high-speed ancestor selection, full-speed nesting and the root integrated TT.

The complete USB suite uses actual guest IRQ/controller/TinyUSB traffic through
engine recording, saved MIDI and playback. Cases include direct HS/FS, sixteen
cables, two nested FS hubs, four simultaneous devices with duplicate serials,
2048-byte SysEx with interleaved realtime, 100 hotplug cycles with bidirectional
traffic after every reattach, retirement and explicit selection of a different-serial
replacement, content-mismatch canary, OUT stall and permanent
NAK. A deployment gate attaches a device before U-Boot, then verifies discovery,
exact reset delivery before network updates, cold reentry and watchdog rollback.

Evidence is under `work/zynq/usb-suite/`, with portable results in
`work/zynq/usb-portable/`. Per-case results retain firmware/emulator hashes,
launch/QMP topology, independent raw USB traffic, serial diagnostics and memory
readings. The read-only queue/input/completion observations synchronize tests after actual
transfers; they never inject into engine MIDI buffers. Direct-load ELF files, test credential inputs, SD images, keys and unredacted
Ethernet captures are private and excluded from USB CI artifacts. Public ELF
evidence uses the generic deployment images, which contain no credential.

The isolated QEMU 10.0.13 build adds a deterministic USB-MIDI fixture and the
ChipIdea port-speed/ULPI register behavior needed to run the production driver.
It exercises functional full-speed connections and hubs, while its EHCI/hub
routing does not reproduce real single-/multi-TT microframe scheduling. Stock
`usb-audio` cannot substitute for this MIDI fixture. Neither model verifies PHY
signaling, VBUS current, silicon DMA/cache coherency or physical latency/jitter.
Libusb passthrough would still use the emulated controller and require physical
devices. The hardware matrix remains a separate acceptance gate.
