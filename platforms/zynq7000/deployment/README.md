# Zynq SD boot and Ethernet deployment

This deployment runs the actual FreeRTOS/AMD lwIP Shoop host on one emulated
Cortex-A9 with **1 GiB PS DDR** and a separate **512 MiB SD card**. Audio/MIDI are
dummy endpoints. The application Rust dependency graph remains `no_std` + `alloc`.
Linux, Vitis, mounts, privileged block access, TAP and KVM are unnecessary.

The boot chain delivered for a board is `BootROM → U-Boot Zynq SPL → U-Boot →
FreeRTOS`. QEMU 10.0.13 executes **U-Boot proper**, seeded at `0x04000000`. Its
BootROM/physical DDR environment is absent. The reference ZC702 SPL builds, but
stops polling PS initialization at SLCR `0xf8000b74` in this model; the emulator
does not certify BootROM loading, SPL handoff or physical DDR training. After
U-Boot entry, GEM traffic, SD controller I/O, image validation, selection, nonce
generation, confirmation and resets all execute in the guest. The harness never
injects an application ELF, slot decision or boot nonce.

## Build and qualify

Install the Debian tool versions in `../pins.json` and the packages in the
`deployment` job of [the firmware workflow](../../../.github/workflows/zynq_freertos.yml).
The loader additionally needs OpenSSL development headers, Bison, Flex, DTC and
Python venv; QEMU needs GLib/slirp development headers and Ninja. Python build
dependencies Meson/pycotap are pinned and installed by `build_qemu.py --fetch`.
Rust is 1.99.1 with `armv7a-none-eabihf`.

From the repository root, this command builds everything and runs all deployment
gates plus the original complete firmware suite:

```sh
python3 platforms/zynq7000/deployment/suite.py --fetch --with-original
```

Use `--directory work/zynq/deployment-second` for a second run using cached build
inputs. Every suite run requires a new output directory and generates new test
keys, credential, cards and state. `--port-base` and `--updater-port` allow an
independent experiment at other loopback ports. CI runs the deployment suite and
the original suite in parallel jobs, and retains the existing native/browser/
portable checks. Each phase owns a bounded process group and cleans it up on
success, failure or timeout.

`artifacts/` contains public loader/firmware files, signed negative test fixtures,
maps and manifests. `evidence/` contains public state summaries, guest serial,
QMP facts/events, CLI transcripts and results. `private/` contains signing keys,
the credential, profiles and raw card images; **do not archive or publish it**.
The artifact builder scans public output for the test credential and private-key
markers. Published packet captures retain timing and network/TCP headers, with
TCP application payloads redacted and checksums updated. Raw captures contain the
trusted-LAN authentication token and stay under `private/packet-captures/`.
Redacted failure captures are retained separately with shorter expiry.

CI adds `--native-coverage` with cargo-llvm-cov 0.8.7 and matching LLVM tools.
The workstation CLI is instrumented during real QEMU deployment/failure cases;
its LCOV report joins the ordinary native report in Codecov. Arm firmware retains
its normal bare-metal build settings. Coverage build data stays in `private/`.

For individual builds:

```sh
python3 platforms/zynq7000/deployment/build_boot.py --fetch
python3 platforms/zynq7000/deployment/build_qemu.py --fetch
python3 platforms/zynq7000/build.py --fetch --app host --deployment --build-id my-build
python3 platforms/zynq7000/deployment/bundle.py \
  --elf work/zynq/build/host-deployment.elf --build-id my-build \
  --key-dir /path/to/signing-key-directory \
  --output work/zynq/my-build.itb --loader-output work/zynq/my-loader
```

The explicit key directory contains `deployment.key` and its RSA-2048 certificate
`deployment.crt`. Keep the private key offline from the target. `bundle.py` embeds
only its public verification key in the loader and signs the FIT configuration
and firmware digest. A device continues trusting the key provisioned in its
loader; signing a bundle with another key does not update that trust. Loader/key
replacement is outside the network updater's write surface.

`my-build.itb` is generic: it contains no device credential or network settings.
The build ID is 1–40 ASCII letters, digits, `.`, `_` or `-`, and must match the
identity compiled into the ELF. Production builds reject fixture symbols and
fault markers. `--deployment-test-fault` produces explicitly named test ELFs;
never deploy those as production firmware.

## Provision a card once

Generate a random 32-byte binary credential using the existing native host tool
or another secure local generator. Keep a workstation copy with private file
permissions; the UI and updater use that same authenticated connection profile.
Provisioning writes a **new regular file**, never an existing card/device:

```sh
python3 platforms/zynq7000/deployment/card.py \
  --output work/zynq/device.img --boot-files work/zynq/my-loader \
  --bundle work/zynq/my-build.itb --build-id my-build \
  --credential-file /path/to/device.key
```

The factory tool generates MBR/FAT32/raw records without mounting anything and
refuses an existing output. It creates `BOOT.BIN` and `U-BOOT.IMG` for SPL. Defaults
are the QEMU network: MAC `02:00:00:00:00:01`, IP `10.0.2.15`, mask
`255.255.255.0`, gateway/TFTP server `10.0.2.2`. Override `--mac`, `--ip`,
`--netmask`, `--gateway` and `--server` for an actual device. Each device needs
unique network identity and credential. A random nonzero counter seed is the
default; `--nonce-seed` exists for reproducible isolated tests. Do not recreate a
deployed device with an old seed/credential combination, which could reuse its
boot authority. Explicit identity reprovisioning requires a new credential and
fresh counter seed.

`layout.json` is the layout authority; `layout.py` generates the C constants.
All addresses below are 512-byte sector LBAs:

| Region | Start LBA | Sectors | Update ownership |
| --- | ---: | ---: | --- |
| MBR/alignment | 0 | 2048 | Factory only |
| Boot FAT32 | 2048 | 131072 | Factory only |
| Metadata/config | 133120 | 8192 | Bounded fixed records |
| Slot A | 141312 | 131072 | Verified inactive slot |
| Slot B | 272384 | 131072 | Verified inactive slot |
| Remaining space | 403456 | 645120 | Unassigned |

Configuration copies are at metadata-relative sectors 0/8, selection at 16/24,
and nonce counters at 32/40. Each record is 512 bytes with magic/version/kind,
zero reserved header and CRC32. Format/handoff **version 2** includes retained
fallback and confirmed-boot failure accounting; version 1 is rejected. Config,
selection and nonce durability are independent. The application writes only the
two selection records; the loader additionally reserves nonce records and writes
only inactive-slot payload/descriptor sectors. Neither can rewrite boot/config
regions through network commands.

## Run the emulator or use a board

The suite's `BootQemu` launcher attaches the regular-file SD image to the real
emulated controller and seeds only the key-bearing U-Boot binary. It requires
the pinned custom QEMU manifest and verifies actual one-CPU/1-GiB QMP facts.
TCP forwards are data `16400` and management `16401`; Fastboot UDP forwards
`16554` to guest `5554`. All host listeners bind loopback. Never let two VMs or a
host writer own the same writable card simultaneously. The suite creates
independent cards per case and retains a card across warm and process restarts.

To keep your own provisioned card running for interactive UI/network updates:

```sh
python3 platforms/zynq7000/deployment/emulator.py \
  --loader work/zynq/my-loader/u-boot.bin --card work/zynq/device.img \
  --credential-file /path/to/device.key
```

It prints the serial-log path, UI command and deployment command, and exports
`work/zynq/deployment-interactive/profile.json`. The credential argument is the
workstation copy used by clients; the guest obtains its credential and boot nonce
from SD. Updater resets keep this emulator process/card alive. Ctrl+C or confirmed
ordinary shutdown closes QEMU while retaining the card. Add `--tftp PUBLIC_DIR`
for a card provisioned with the one-shot RAM-boot request.

For RAM development boot, provision with `card.py --network-boot` and place a
signed bundle at `shoop.itb` in the launcher's public TFTP directory. Export only
public artifacts, never a signing-key directory or raw card. U-Boot consumes the
one-shot request, pulls and verifies the FIT through GEM, and starts FreeRTOS
with an SD-reserved fresh nonce. A missing server has bounded retries and falls
back to confirmed SD boot. Subsequent ordinary boots require no server. RAM boots
do not confirm or replace an SD slot.

For physical hardware, replace the reference PS initialization and platform
network assumptions with the board's exported hardware setup. Initial SD
programming, boot-mode straps, DDR training, GEM PHY wiring, clock/reset behavior,
cache/DMA coherency and physical card durability require board validation. The
repository intentionally provides no command that writes an arbitrary host block
device. Real audio and FPGA programming remain outside scope.

## Update over Ethernet

Build the workstation tool:

```sh
cargo build --locked -p shoop_host --no-default-features --bin shoop-deploy
target/debug/shoop-deploy --profile /path/to/profile.json status
target/debug/shoop-deploy --profile /path/to/profile.json deploy work/zynq/my-build.itb
```

On QEMU add `--updater 127.0.0.1:16554`; a board defaults to UDP port 5554 at the
profile's management host. Profile fields match the existing native remote
profile: name, session version, data endpoint, management endpoint, credential
path and network settings. Relative credential paths resolve against the profile.
Normal UI connections retain strict session-version validation. Deployment uses
its own stable, bounded management envelope on the same authenticated listener;
its compatibility does not depend on the UI/session version.
Native host profiles omit `credential` and use keyless discovery. Board profiles
retain the explicit credential path; failed board authentication does not retry
through the native keyless protocol.

The command authenticates, refuses competing ownership, binds a request to the
current boot nonce and unique operation ID, stops rendering, acknowledges all 32
MIDI resets, and finishes stopped-resource cleanup. Only then does the guest
durably record the update request and reset into U-Boot. Cleanup or metadata
failure produces a refusal and never requests reset. Ordinary shutdown still
returns normally. Native hosts without this capability report unsupported.

The CLI matches U-Boot's durable operation cookie, transfers a bounded FIT using
Fastboot UDP, requests verified installation in the inactive slot and reboots.
Success means the new running digest is **durably confirmed**, rather than merely
transferred. It reports rollback/refusal/deadline errors distinctly. Default
deadline is 300 seconds, adjustable with `--timeout 1..3600`.

The adjacent `profile.json.deployment.json` journal contains only public operation
identity, target/digest and expected boot identity. `--journal` selects another
path. Rerun the same command after a lost reply or interrupted workstation process;
the operation cookie and verified-image checks reconcile its state. A completed
journal permits the next image/update request. A pending journal for another
image/target is refused; inspect and reconcile that operation before replacing
the journal. Do not treat a deadline as evidence of failed installation: inspect
`status` and rerun with the same bundle. Restoring/reprovisioning a card is an
explicit recovery action, not an automatic CLI fallback.

## Failure and recovery policy

U-Boot verifies the signed target, digest, size, and ELF load/entry ranges both
before installation and before execution. Bundles are limited to 32 MiB, staged
at 896–928 MiB; ELF segments must stay in the platform 1–128 MiB or Rust
128–896 MiB ranges. An LMB guard rejects TFTP blocks before they can cross the
32 MiB staging bound, including when a server supplies an oversized image.
Loader relocation/stack and its second verification buffer
remain in the upper reserved 128 MiB. The low-memory handoff is at `0x8000`.

A candidate is published only after payload/descriptor write/readback verification.
It gets two durably consumed trials. Health requires valid identity/config,
initialized service/listeners and observed render/broker progress, then redundant
SD confirmation. The previous confirmed image remains as fallback after promotion.
If a newly confirmed image hangs repeatedly, three failed boots return to that
retained fallback. Sixty seconds of stable task progress or a clean shutdown
clears its boot failure streak. Updating again explicitly retires the inactive
fallback before overwriting it while retaining the currently confirmed slot.

The Cadence system watchdog is armed before entering FreeRTOS. It is fed only
after control, render and broker progress; stopped render ownership is explicitly
retired during cleanup. Pre-ready hangs get a 30-second timeout and later stalls
10 seconds, measured in guest virtual time. QEMU's pinned Zynq model lacks this
watchdog, so `qemu-cadence-watchdog.patch` adds its register/timer/reset model.
Tests require actual QMP WATCHDOG/guest RESET events and automatic rollback; a
host-script process restart does not pass those cases.

One damaged selection/counter/config copy can be recovered. Conflicting valid
configuration copies are refused. The loader advances and reads back both nonce
copies before **every** application entry; rollback never rolls the counter back.
Unknown/wrapped identity, unusable configuration or both unusable state copies
stay in recovery. A verified image can reconstruct selection state without
resetting configuration/counters; unusable identity/config needs explicit local
reprovisioning. No valid image also enters the updater without requiring Shoop.

For an already-running recovery updater:

```sh
target/debug/shoop-deploy --updater DEVICE_IP:5554 updater-status
target/debug/shoop-deploy --updater DEVICE_IP:5554 stage signed-image.itb --reboot
target/debug/shoop-deploy --profile profile.json status
```

`stage` reports **pending**, never confirmed success. Recovery transport is the
explicitly trusted LAN; the provisioned loader's signature key authorizes firmware.
It exposes no arbitrary flash/erase, shell execution, bootloader or config writes.
Network recovery cannot repair destroyed boot media or replace the loader/key.

`fault_disk.py` is test-only NBD backing storage under the real SD device: it
records exact guest write offsets/hashes and can commit 199 bytes, omit a write,
or pause a write before simulated power loss. No host edits a live guest's card.
The tests also cover malformed bundles, corrupted/unsupported metadata,
nonce exhaustion, interrupted CLI/download sessions, seeded packet loss,
confirmation failures and task stalls. These establish the stated emulation
fault model; physical SD flash atomicity/durability remains a hardware test.

## Source changes and licensing

U-Boot v2026.10 and QEMU 10.0.13 are pinned to exact commits. Shared boot-state C
and loader/runtime integration carry GPL-2.0-or-later notices, compatible with
their U-Boot use and the application repository. Preserve upstream U-Boot, QEMU,
AMD, FreeRTOS and lwIP notices with redistributed sources/binaries.

`uboot-network-deployment.patch` fixes server-side ARP reception, abandoned UDP
download cancellation on fresh INIT, duplicate sequence handling at wrap, and
adds a board command-policy hook. Shoop's allowlist refuses stock Fastboot `boot`,
`set_active`, alternative reboot modes and arbitrary writes/shell commands.
The custom command supplies signature/load checks, bounded SD writes, boot policy
and handoff; stock Fastboot `set_active` does not supply this A/B policy.
Build manifests record source/config/patch/input/artifact hashes and tool versions.
The original direct-load firmware tests continue using stock QEMU independently.
