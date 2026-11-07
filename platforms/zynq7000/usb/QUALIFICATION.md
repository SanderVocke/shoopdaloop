# ZC702 USB MIDI physical qualification

Physical qualification is pending. The user asked to defer it until the software
and automated evidence are ready. Simulation results do not complete this list.

## Prepare the board

Use the ZC702 reference PS initialization and a 1 GiB DDR profile. Record the board
revision, PS initialization source/hash, firmware Git head, ELF/build-manifest
hashes, signing-key identity (public certificate only), and native client version.
Use the existing provisioning/deployment guide for the SD card and credentials.
Never include credentials, signing keys or raw authenticated Ethernet captures in
public evidence.

Connect the PS USB0 ULPI port with a host/OTG adapter that supplies the host ID
connection. Follow the board revision's user guide for USB host power/jumper
settings. The firmware owns USB0, resets the PHY through MIO7, configures ULPI on
MIO28–39, checks PHY identity/scratch registers, and requests external VBUS drive.
USB1 remains available to other designs. Start with a self-powered instrument or
a powered hub; verify VBUS on the connector before attaching bus-powered devices.
Record the adapter, hub power supplies and any device that back-powers VBUS.

Capture UART from power-on through shutdown. Required startup markers include
`SHOOP_USB_PHY`, `SHOOP_USB_READY`, `SHOOP_USB_MIDI_MOUNT` and `SHOOP_HOST_READY`.
A missing PHY or failed register handshake must report failure within its bounded
initialization path while network control remains available.

## Device and hub matrix

For every row, retain device/configuration/string descriptors, VID/PID, serial
presence, interface and cable counts, negotiated speed and the complete physical
hub topology. Record hub protocol/TT mode from descriptors rather than infer it
from marketing labels. Test devices present at cold boot and attached after the
service is running.

- [ ] Direct full-speed keyboard/controller: discover, select, record and replay.
- [ ] Bidirectional instrument/synth: exact IN and OUT messages.
- [ ] USB-to-DIN interface with DIN OUT wired to DIN IN: exact loopback capture.
- [ ] A high-speed MIDI device, directly connected.
- [ ] Full-speed hub with downstream MIDI devices.
- [ ] High-speed single-TT hub with full-speed and high-speed downstream devices.
- [ ] High-speed multi-TT hub with simultaneous downstream traffic.
- [ ] Two external hub tiers, within two hubs in aggregate.
- [ ] Four non-hub devices operating concurrently.
- [ ] Sixteen logical inputs and sixteen outputs in aggregate; test all cable IDs.
- [ ] Composite audio+MIDI device and multiple MIDIStreaming interfaces.
- [ ] Input-only and output-only MIDI interfaces.
- [ ] Duplicate serials and devices without serials remain distinct by topology.
- [ ] Unsupported device and excess device/interface/cable capacity: explicit
      refusal, with admitted devices continuing to operate.

USB MIDI-to-DIN converters own their DIN serialization. A USB completion proves
USB transport delivery; use a separate DIN analyzer/capture to measure actual DIN
completion and latency.

## End-to-end workflow

Use the actual native connection dialog, with no desktop MIDI service proxying
traffic to the firmware. Record the visible product/topology/interface/cable
labels and selected routes. Disable input monitoring when the DIN loopback would
otherwise create a feedback route.

- [ ] Record and replay notes, velocity, control/program changes, pressure,
      pitch bend, system-common and realtime messages in both directions.
- [ ] Compare exact 2-, 3-, 4-, 5- and 2048-byte SysEx messages, including endpoint
      and FIFO boundaries; realtime interleaved with SysEx remains independent.
- [ ] Oversized/incomplete SysEx and malformed packets produce bounded recovery,
      correct counters and no traffic on another cable/device.
- [ ] Save/reload and round-trip recorded MIDI/session media through the native
      client. Reorder enumeration while retaining physical identities/routes.
- [ ] Disconnect/reconnect the UI while instruments and the renderer continue.
- [ ] Remove/reinsert one device during traffic, then remove an entire hub.
      Preserve unrelated routes; refuse stale address/generation traffic.
- [ ] Perform 100 hotplug cycles with bidirectional traffic after every reattach.
      Record stable Rust/C heap use, task counts and stack margins.
- [ ] Exercise output backpressure, endpoint stalls and an unresponsive device.
      Confirm explicit loss/failure, bounded recovery and continued control.
- [ ] Shutdown drains all 32 reset messages on every live output before cleanup
      confirmation. Unresponsive output produces a bounded refusal, while the
      control heartbeat remains live.
- [ ] Cold boot, warm boot, successful network update, updater entry and rollback
      with devices connected. Confirm PHY reset/reinitialization, fresh discovery,
      credential/authority behavior and safe USB DMA quiescence.

Measure IN/OUT and DIN latency and jitter with an external clock/analyzer under
simultaneous MIDI, rendering and Ethernet load. Retain distributions and maxima,
not only averages. Record renderer xruns/discontinuities, MIDI accepted/completed/
refused/dropped/recovery counters, heap high-water marks and all task stacks.
Inspect sustained VBUS power and cache/DMA coherency under the same load.

## Evidence record

For each case, record date, operator, board/firmware/client identities, topology,
message corpus/hash, independent observed traffic/hash, expected/actual result,
UART/counter/memory evidence and latency measurements where applicable. Mark a
row complete only after its physical result is retained. Link failures to the
reproduction and fix, then rerun affected rows on the new firmware head.
