# Zynq network deployment and emulated SD boot

## Status and delivery

Execution authorized on 2026-11-03. Plan written using `.agents/prompts/write_plan.md`.
Implementation is in progress. Prerequisite: the completed
`PLAN_ZYNQ_FREERTOS_HOST.md` and [PR #914](https://github.com/SanderVocke/shoopdaloop/pull/914).
The inspected host head is `d58beffcd6d4e60a1d75a4ff29587c1f28458480`;
`feat/backend-replacement` is at `3aabf437`. PR #914 is currently open.

Implement on a new `feat/zynq-network-deployment` branch containing the host
prerequisite. Deliver a separate, unmerged PR targeting **`feat/backend-replacement`**.
If #914 is still open, document the shared prerequisite commits; reconcile its
eventual merge without merging either PR on the user's behalf.

## Goals and scope

Provide a reproducible deployment boot chain:

`BootROM → first-stage loader → U-Boot → Shoop FreeRTOS host`

After one initial SD provisioning, a workstation can network-boot a development
image or install a persistent firmware update over Ethernet. Normal startup must
work without a network boot server. A failed/interrupted update must preserve a
working image and a network recovery path.

Use upstream U-Boot for Zynq GEM networking, TFTP, Fastboot over UDP, MMC and image
verification. Supply the Shoop-specific handoff, update policy and host tooling.
Use a **512 MiB file-backed emulated SD card**, independently of the existing
**one-core, 1 GiB PS DDR** contract. The deployed application remains FreeRTOS +
AMD lwIP with dummy audio/MIDI and target-side Rust `no_std` + `alloc`.

Include initial card-image creation, a buildable reference first stage, network
RAM boot, local SD boot, configuration/credential provisioning, authenticated
reboot into the updater, A/B updates, boot confirmation, watchdog recovery,
interruption tests, documentation and CI. Linux on the RT host is unnecessary.

Physical-board testing, real DDR training/BootROM certification, QSPI/eMMC update
backends, remote replacement of the first-stage loader/U-Boot, FPGA bitstream
deployment, real audio, fleet management and general-purpose OTA services are
outside scope. TFTP is the required pull transport; HTTP/USB are future options.
The network policy remains an explicitly trusted LAN.

## Immutable acceptance criteria

1. **Boot chain and provenance:** pinned sources/tools produce the reference
   first-stage loader, U-Boot, firmware update bundle and provisioned SD image.
   QEMU boots real U-Boot on `xilinx-zynq-a9`, one CPU and exactly 1 GiB RAM.
   Document the earliest stage QEMU actually executes and the stages it bypasses.
2. **Network RAM boot:** U-Boot fetches and validates a Shoop image through the
   emulated GEM network, loads it into RAM and hands over to the actual FreeRTOS
   host. Native-client attachment, rendering progress and confirmed shutdown pass.
   Host-side injection of the application ELF or its boot nonce cannot pass this gate.
3. **Persistent SD boot:** U-Boot reads firmware and deployment state through the
   emulated SD controller. A fresh QEMU process using the same updated card boots
   the installed image without TFTP/Fastboot or host-side SD edits. The image
   builder and normal deployment never require mounting a card or privileged I/O.
4. **Convenient network update:** one workstation command authenticates to a
   running host, requests clean updater entry, transfers a versioned image over
   Ethernet, installs only the inactive slot, boots it and reports success only
   after durable application health confirmation. A known-good fallback remains.
5. **Recoverability:** interrupted transfer/write/activation/confirmation,
   malformed or wrong-target images, failed trial boots, application hangs and
   damaged metadata follow a tested, bounded recovery policy. A failed candidate
   rolls back automatically; if no valid slot/state remains, U-Boot offers network
   recovery. Recovery must work when the application cannot run.
6. **Identity, ownership and RT behavior:** every application entry receives a
   fresh nonzero boot nonce, including reset and rollback. Deployment preserves
   device credentials/configuration and refuses stale/competing authority.
   Ordinary shutdown retains its existing semantics. No SD/update I/O, allocation
   or boot-state work enters rendering; the 1 GiB and existing resource bounds hold.
7. **CI and delivery:** PR CI executes network boot, SD boot, successful update,
   rollback and interruption tests with bounded child processes and retained
   evidence. Existing full firmware/native/browser/portable gates remain enabled
   and green on the final PR head. Inspect and address arriving automated review;
   leave the PR unmerged and the worktree clean.

## Design rules

### First stage, boot memory and handoff

- Prefer a released mainline U-Boot with its Zynq SPL as the reference first stage
  performing the FSBL role. Pin the exact commit and configuration. A pinned AMD
  FSBL may replace SPL if a demonstrated compatibility requirement warrants it;
  record the reason and build inputs. Deliver the selected SD boot files, e.g.
  `boot.bin` + `u-boot.img` for SPL, or `BOOT.BIN` for an AMD FSBL chain.
- Reuse the current compiler/AMD/QEMU pins where compatible. Pin any additional
  cross compiler, U-Boot tools, Fastboot client and image-generation dependencies.
  Build without a Vitis installation; pin and hash licensed upstream reference PS
  initialization inputs and distinguish these from an eventual physical board's
  exported setup. The pinned upstream source carries the 12,814-line generated
  reference input; avoid a second vendored generated copy.
- First prove boot/reset/SD support on pinned QEMU 10.0.13. Newer QEMU documentation
  is guidance, not evidence that this version implements identical boot-mode or
  watchdog behavior. Version changes or narrowly scoped model adaptations need
  recorded evidence and reproducible pins.
- QEMU may seed the earliest supported loader because it lacks the physical boot
  environment. Prefer a first-stage handoff test when executable in the model;
  label direct U-Boot entry explicitly. Subsequent firmware selection, nonce
  generation, SD operations and updater state transitions must execute in the guest.
- Preserve the host's low-memory reservation, platform below 128 MiB and Rust
  arena at 128–896 MiB. Bound update bundles to 32 MiB. Budget U-Boot relocation,
  malloc, stacks, FDT and transfer/ELF staging within the upper 128 MiB before
  loading the host; this region stays unavailable to host allocators. Assert all
  live ranges, ELF file/memory sizes and entry points before copying or clearing
  sections. Account for the 768 MiB NOLOAD arena and self-overwriting ELF loads.
- Introduce a versioned, validated handoff in reserved low memory, retaining the
  nonce at `0x8000`. Include board/profile identity, slot/bundle identity, network
  configuration, credential and boot-state references. Quiesce GEM DMA, timers,
  interrupts and caches/MMU as required before entering the host; reinitialize
  SD and networking ownership on each side. Never return to an overwritten loader.

### SD layout and persistent state

Use a machine-readable, sector-aligned layout shared by the image builder,
U-Boot and the firmware. Initial layout, finalized before implementing writes:

| Region on 512 MiB card | Budget | Ownership |
| --- | ---: | --- |
| MBR/alignment | 1 MiB | Factory provisioning |
| Boot FAT partition | 64 MiB | First stage/U-Boot; immutable to network updater |
| Raw deployment metadata/config partition | 4 MiB | Bounded boot/runtime metadata API |
| Raw firmware slot A | 64 MiB | Validated bundle plus slot descriptor |
| Raw firmware slot B | 64 MiB | Validated bundle plus slot descriptor |
| Remainder | Unassigned | Never an implicit update target |

- Generate the card entirely as a regular file, including partitions and FAT
  contents. Use independent card copies per test and retain a card unchanged
  across guest warm resets and complete process restarts. No shared writable card
  between concurrent QEMU instances; no host writer while the guest owns it.
- Share a versioned, fixed-size metadata codec/state machine between bootloader,
  firmware and tooling through common definitions and cross-language golden
  vectors. Use bounds, checksums, generation numbers, redundant records and
  write/readback ordering; do not assume FAT rename or one SD-sector write is atomic.
- Separate configuration, boot-nonce durability and A/B selection records.
  Provision a per-device counter seed; advance and read back redundant counter
  copies before every application handoff. Rollback must never roll back this
  counter. A single damaged copy must not permit nonce reuse; if freshness cannot
  be established, stay in recovery. Check wrap and never reset identity on update.
- Keep a confirmed slot, optional pending candidate, bounded attempts, bundle
  digests and operation identity. Mark a candidate pending only after a complete
  verified write/readback; durably consume an attempt before jumping. Retain the
  confirmed image until the candidate confirms itself. Partial metadata changes
  select the last valid committed state or enter recovery explicitly.
- The FreeRTOS side needs a minimal bounded raw-sector SD interface, likely using
  pinned AMD `xsdps`; a guest filesystem is unnecessary. Perform boot-state I/O
  outside rendering. Handle failed/timed-out writes without false confirmation.
  Do not silently add `std` to target dependencies.

### Bundle, provisioning and deployment protocol

- Package a board/profile identifier, version/build identity, entry/load contract
  and firmware digest in a bounded FIT-based bundle. Use U-Boot's verification
  support with an explicit signing-key input and a public key embedded in the
  provisioned loader. CI generates disposable test keys; production private keys
  are never committed or archived. Reject invalid signatures, lengths, address
  ranges and wrong-target bundles before activation and again before execution.
- Explicitly connect FIT verification to ELF loading/handoff; plain `bootelf`
  does not automatically authenticate a signed container. Fastboot's standard
  Android boot flow and `set_active` stub are not the Shoop boot/rollback policy.
- Provision device MAC/IP/network settings and the existing 32-byte credential
  separately from generic update bundles. Pass validated values to the host;
  update artifacts must not contain a device's live credential. Firmware updates
  preserve boot files, configuration, keys and counter records. Never silently
  reformat a mismatched or damaged card.
- Use Fastboot UDP for workstation push and TFTP for development pull. Export
  only bounded inactive-slot/update operations: do not expose arbitrary MMC
  writes, partition-table/bootloader replacement or an unrestricted U-Boot shell.
  Validate a staged image before publishing it as bootable. In recovery with no
  confirmed slot, a verified initial installation may populate a designated slot.
- Add explicit deployment capability/status and reboot-to-updater behavior to
  authenticated management, using existing authority and operation-id rules.
  Ordinary native hosts may report unsupported. Version shared messages correctly
  and preserve normal shutdown; do not create a Zynq-only fork of the session wire
  format. Refuse competing ownership and unsupported/busy operations clearly.
- Clean updater entry stops rendering, acknowledges all MIDI reset messages and
  completes cleanup before durably recording the request and resetting. A failed
  cleanup cannot trigger reset. Lost replies are reconciled by operation identity
  and observed boot/update state, not by replaying unsafe writes.
- The deployment CLI drives request, discovery/reconnect, transfer, verification,
  trial boot and confirmation with deadlines and useful progress. It can resume
  after interruption and explicitly distinguish installed, trial, confirmed,
  rolled back and recovery states. Serial is a diagnostic/manual recovery aid;
  the normal deployment command must need only Ethernet and the supplied profile.

### Health and failure model

- A candidate confirms only after validating its handoff/config, starting service
  and endpoints, observing render progress and persisting the success record.
  Publish the running bundle/slot and confirmation status for the workstation.
  Confirmation cannot depend on a workstation remaining connected.
- Configure bounded trial attempts and a watchdog/reset path. Feed it only when
  required task progress is observed; an idle timer must not hide a stuck renderer
  or control task. A hang before health confirmation must return to U-Boot and
  eventually the confirmed slot. Demonstrate the modeled reset path rather than
  substituting a host-script restart for the watchdog test.
- Local confirmed-image boot has no server dependency. No valid image, exhausted
  candidates or unusable identity state enters a documented network recovery
  mode; corrupted counters/configuration require explicit reprovisioning where
  safe automatic repair is impossible. Network recovery does not require Shoop
  to start. Bootloader replacement and destroyed boot media require local recovery.
- Emulated power loss proves the specified image/write fault model, not physical
  SD controller durability or flash behavior. Record exactly which guest writes
  were committed, torn or omitted; test both warm reset and cold process restart.

## Staged implementation

### Stage 0 — Establish baseline and freeze deployment contracts

- [x] Fetch the actual base/prerequisite state, preserve unrelated changes and
  create `feat/zynq-network-deployment`. Carry this plan onto the branch; record
  SHAs and PR dependency. Preserve the completed host plan as historical evidence.
- [x] Pin the first-stage/U-Boot source and build tools; resolve the reference
  1 GiB PS/SD configuration and verify QEMU reset/SD/watchdog capabilities.
  Freeze SD layout, metadata/handoff versions, transfer bounds and failure policy.
- [x] Identify exact integration points in `platforms/zynq7000/{build,qemu,suite}.py`,
  `host.c`, `shoop_host_freertos` and authenticated host management. Re-run the
  existing three-gate suite and focused management/transport tests; commit baseline.

Verification: build provenance and write/ownership boundaries are reviewable;
no unresolved boot/reset capability is hidden behind a promised later workaround.

### Stage 1 — Build the loader and provision a real emulated SD card

Depends on Stage 0.

- [x] Add deterministic first-stage/U-Boot builds and required Zynq configuration,
  producing the selected boot files, maps and manifests. Keep source patches
  separate, justified and hashed. Build the first stage even if CI must enter
  U-Boot directly; add the executable first-stage handoff check where supported.
- [x] Add regular-file SD image generation/provisioning with explicit keys/device
  configuration and initial A image. Extend the launcher for SD, bootloader entry,
  TFTP service, UDP forwarding and complete serial/QMP/reset evidence. Restrict
  host listeners to loopback; retain existing direct-load firmware development mode.
- [x] Prove U-Boot GEM exchange and guest SD read/write/readback across a fresh
  QEMU process, including correct card geometry, protected partition boundaries
  and a default local boot path. Commit the first deployment milestone.

Verification: U-Boot accesses the actual emulated controllers; no host process
performs guest SD reads/writes on its behalf. First-stage build evidence and
QEMU entry-point coverage are reported separately.

### Stage 2 — Deployment gate 1: network and local boot through U-Boot

Depends on Stage 1.

- [x] Implement shared metadata validation, durable boot-nonce allocation and
  device provisioning/handoff. Integrate runtime network/credential configuration
  and the bounded FreeRTOS SD metadata interface. Keep direct-load tests explicit.
- [x] Add bundle creation/verification and bounded ELF load/handoff. Prove TFTP
  RAM boot through GEM and local boot of slot A through SD, with nonce/config
  supplied exclusively by U-Boot. Enforce all loader and firmware memory ranges.
- [x] Execute authenticated native-client smoke and the actual UI workflow after
  loader handoff. Cold boot without a TFTP server and warm reset at identical
  endpoints; verify fresh authority and exact running bundle identity. Commit.

Verification: real FreeRTOS networking/rendering starts from both image sources,
old authority is refused, and the tests cannot pass by injecting the app ELF,
nonce or slot selection from the host launcher.

### Stage 3 — Deployment gate 2 foundation: A/B state and health confirmation

Depends on Stage 2.

- [x] Implement bounded trial selection, durable attempt accounting, independent
  nonce progression, application health confirmation and confirmed-slot fallback.
  Share the authoritative state transitions across loader/runtime tooling.
- [x] Add supported watchdog/reset integration and updater-entry persistence.
  Test pre-ready crashes and post-start task stalls, success confirmation and
  cold boot of the last confirmed image without workstation involvement.
- [x] Test metadata codecs, torn-record selection, single-copy damage and boundary
  conditions; confirm safe recovery when both copies/configuration are unusable.
  Compare card regions before/after operations and commit the state-machine milestone.

Verification: confirmation is a guest durable write, failed trials terminate,
rollback retains fresh boot identity, and watchdog recovery uses a guest reset.

### Stage 4 — Deployment gate 2 completion: one-command Ethernet updates

Depends on Stage 3.

- [x] Add capability-aware authenticated updater entry, lost-reply reconciliation
  and explicit status reporting. Prove ownership refusal, cleanup failure refusal
  and unchanged ordinary shutdown across native and FreeRTOS hosts.
- [x] Configure Fastboot UDP with restricted staging/commit operations and add
  workstation deployment/status commands. Preserve the confirmed slot, boot files
  and device configuration; check readback and signatures before marking pending.
- [x] Update A → B and B → A using distinguishable firmware identities. Have the
  CLI observe durable confirmation, then disconnect the workstation and cold boot
  the installed image using only the retained card. Document commands and commit.

Verification: a single operator command completes each network-only deployment;
the transferred image actually runs, and success is reported after confirmation.

### Stage 5 — Deployment gate 3: interrupted updates and autonomous recovery

Depends on Stage 4. Add deterministic cut points plus a small seeded randomized
set; preserve replay seeds and the affected SD records for failures.

- [x] Cover the matrix below with actual QEMU execution and card persistence.
  Use explicit torn/omitted-write injection in addition to killing QEMU; a process
  kill alone does not prove every storage failure boundary.
- [x] Test interrupted/resumed CLI sessions, lost UDP replies, slow/no server,
  stale authority after rollback, attempted protected-region writes, incompatible
  metadata/handoff formats and counter overflow. Preserve useful error distinctions.
- [x] Demonstrate network installation from recovery with no functioning app,
  bounded watchdog rollback, and confirmed-image boot when the workstation is
  absent. Recheck memory/stack bounds and render-path allocation guards; commit.

| Failure point | Required observable result |
| --- | --- |
| Download, inactive-slot erase/write, readback | Confirmed image still boots; incomplete slot is not selected |
| Candidate publication or attempt-record write | Valid previous state or bounded trial; never execute unverified bytes |
| Candidate starts but never confirms, crashes or hangs | Watchdog/reset, bounded attempts, automatic confirmed-slot rollback |
| Success-record write interrupted | Valid confirmation or safe retry/fallback; no false success |
| One metadata/counter record damaged | Valid state recovered without nonce reuse or rewriting protected regions |
| Both state copies unusable / both images invalid | Explicit network recovery; no fabricated confirmation or boot identity |

Verification: failures have guest evidence and precise outcomes, not merely a
host timeout. The invariant is a usable confirmed image or an honest recovery mode.

### Stage 6 — Permanent CI, final validation and PR delivery

Depends on all previous stages.

- [x] Add bounded Linux PR CI for loader builds, SD generation, TFTP/local boot,
  network A/B update, watchdog rollback and representative interruption cases.
  Keep broader seeded fault coverage separately invocable/scheduled if necessary.
  Cache reproducible build inputs only; never reuse mutable cards, keys or state.
- [x] Retain compact manifests, boot/config versions, image hashes, QMP CPU/RAM
  facts, loader/runtime serial logs, deployment transcripts and result records.
  Keep redacted metadata/card diagnostics and packet captures for failures;
  never archive live credentials/private keys. Provide a single full validation command.
- [x] Run that command from fresh and cached builds with two distinct signed
  firmware identities. Exercise factory SD boot → update → health confirmation →
  server-free cold boot → failed candidate → automatic rollback → network recovery.
  Run the original full host suite and required native/browser/no_std, formatting,
  warning-denying build, tracing/boundary and applicable test-attribute checks.
- [ ] Review final diffs, source/licenses, memory/SD maps, provisioning and recovery
  docs against every criterion. Push the milestone commits; create/update the PR
  onto `feat/backend-replacement`. Resolve base movement and automated review,
  repeat affected checks and obtain green required/applicable CI on the final head.
  Link evidence in the PR; leave it unmerged and the worktree clean.

Verification: the documented operator workflow works through Ethernet and the
emulated SD card end to end; first-stage and physical-board limitations are stated
precisely, and the final PR contains reproducible passing evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Primary references

These explain the available components; implementation must pin actual source
revisions rather than treating moving documentation as a reproducible input.

- [U-Boot Zynq support and SPL/SD boot files](https://docs.u-boot-project.org/en/v2026.10/board/xilinx/zynq.html).
- [Fastboot UDP, storage backends and custom board operations](https://docs.u-boot-project.org/en/latest/android/fastboot.html).
- [ELF launching](https://docs.u-boot-project.org/en/latest/usage/cmd/bootelf.html)
  and [FIT verification](https://docs.u-boot-project.org/en/latest/usage/fit/signature.html).
- [Boot-count fallback and application confirmation responsibility](https://docs.u-boot-project.org/en/latest/api/bootcount.html).
- [QEMU Zynq devices and loader/SD examples](https://www.qemu.org/docs/master/system/arm/xlnx-zynq.html),
  currently a newer model/manual than the project's pinned QEMU; verify capabilities.

## Execution evidence

- Created `feat/zynq-network-deployment` from `d58beffc`; prerequisite #914 remains
  open against `feat/backend-replacement` at `3aabf437`.
- U-Boot v2026.10 resolves to `5508406582f6f7120dce0c4b3059819a41788db2`.
  Existing GCC 14.2.1 builds U-Boot proper and its Zynq SPL without Vitis.
- On stock QEMU 10.0.13, U-Boot identifies a ZC702 with 1 GiB DDR, fetches 2048
  bytes over real GEM/TFTP, writes/reads the 512 MiB SD card (CRC `9f5edd58`) and
  restarts itself through the guest SLCR reset register.
- Source inspection confirms that QEMU 10.0.13 leaves the Cadence system
  watchdog unimplemented and treats the private watchdog as an interrupt timer.
  A narrow, pinned Cadence watchdog model is being added for autonomous reset
  testing, including hangs with CPU interrupts disabled. Original firmware tests
  continue to use stock QEMU; deployment tests record the model patch provenance.
- The baseline full firmware suite passes all seven phases (716 seconds), and
  all 36 focused connection/transport/host tests pass. The shared C boot-state
  tests pass under AddressSanitizer and UndefinedBehaviorSanitizer.
- The watchdog-enabled QEMU is built from upstream `98f88efe` plus the checked-in
  Cadence model patch. MMIO programming and virtual-time advancement produce
  QMP `WATCHDOG` and guest `RESET` events without a host restart request.
- Stage 1 builds pinned SPL/U-Boot, signed FIT bundles and key-bearing loader
  files. The regular-file card builder creates MBR/FAT32 without mounts; standard
  `mdir` independently reads its boot files. Loader update commands expose no
  raw flashing or remote shell, and the target checks FIT signatures/profile and
  ELF bounds before loading.
- A preliminary signed bundle containing the existing host ELF boots through
  U-Boot and SD, passes the native-client workflow, and cleans Rust/socket/netconn
  counts to zero. Two fresh processes using the same card advance guest-persisted
  nonce records from 102 to 103 and refuse previous-boot authority. This is loader
  integration evidence, not yet the generic credential/health-confirmation gate.
- The reference SPL executes but stalls in its PS initialization `ioread` poll
  (PC `0x404`, SLCR address `0xf8000b74`). QEMU deployment CI therefore enters
  U-Boot proper; it does not claim BootROM or physical PS/DDR initialization.
- Stage 2 generic firmware has no `shoop_credential` symbol. Its validated
  handoff supplies the provisioned token/MAC/IP; pinned AMD `sdps` reads and writes
  raw metadata outside rendering. Health confirmation follows endpoint startup
  and two guest seconds of observed render/broker progress.
- The metadata format is now version 2. It additionally retains the previous
  confirmed image and a boot-failure streak, allowing rollback after crashes
  shortly after confirmation. Three unsuccessful boots select that fallback or
  recovery; 60 guest seconds of stable operation or clean shutdown clear the
  streak. Reusing a slot first clears its old fallback reference durably. Version-1
  prototype cards are explicitly rejected rather than silently resetting identity.
- The Stage 2 network probe transfers 4,362,471 bytes through GEM/TFTP and runs
  from RAM (slot `4294967295`, nonce 8001); the next process boots SD slot A with
  nonce 8002 without a TFTP server. Warm reset advances 9001 to 9002. Previous-boot
  authority is refused. The actual native UI settings/edit/record/play/reattach/
  save/load/shutdown workflow passes on deployed firmware.
- All Stage 2 probes require zero final Rust allocations/sockets/netconns and
  unchanged arena/RTOS bounds. `RUSTFLAGS="-D warnings" cargo build --workspace`
  and hard-float generic firmware builds pass. U-Boot legacy networking requires
  explicitly synchronizing live IP state after programmatic environment changes;
  the test refuses an unintended SD fallback when network RAM boot is required.

- Network deployment milestone: `shoop-deploy` authenticates on the existing
  management carrier, binds updater entry to the boot nonce and operation ID,
  waits for render/MIDI cleanup, and reconciles Fastboot state using its durable
  operation cookie. A separate bounded deployment envelope keeps session protocol
  version 11 unchanged and permits updates across future session versions.
- Real one-command A → B → A completed with durable confirmation at nonces
  14002 and 14003; a fresh QEMU process booted A from the same card without
  an update server. Evidence: `work/zynq/deployment-rpc-roundtrip-2.log`.
- Actual guest watchdog resets recovered from pre-ready failure (two trials) and
  render/control/broker stalls (three boots after confirmation), always returning
  to A with fresh nonces. Evidence: `work/zynq/deployment-watchdog-candidates.log`.
- Pinned U-Boot required three narrow networking fixes: allow server-side ARP
  broadcasts, cancel abandoned Fastboot downloads on a fresh INIT, and preserve
  duplicate detection across 16-bit sequence wrap. The provisioned MAC must
  explicitly replace U-Boot's generated address; its convenience setter refuses
  to replace an existing address. All changes are recorded in source/provenance.
- C/Python fixed-record golden vectors and every single-bit corruption position
  pass. Authenticated management tests cover busy/stale/idempotent requests,
  cleanup failure, independent deployment versioning and native unsupported status.
  Storage fault injection and permanent qualification/CI remain in progress.

- Design refinement: retain the GPL reference PS initialization in the pinned
  U-Boot source rather than duplicating 12,814 generated lines. Explicit hashes of
  its implementation/header are checked against `pins.json` and included in build
  provenance. This preserves reproducibility/licensing and makes board-specific
  replacement explicit. Goals and acceptance criteria are unchanged.
- Delivery opened as draft PR #924 onto `feat/backend-replacement`; initial CI
  includes the new deployment job and all existing gates. Fault qualification
  found an invalid-configuration loader path that attempted app entry; it is
  being fixed to remain in recovery before consuming an application nonce.

- Full native regression: 2,544 tests passed, five intentional skips, with two
  nextest workers while QEMU ran. An existing scheduler-bounded RT stress test
  had failed under the first concurrent four-worker run; it then passed 20/20
  isolated repetitions. No RT-runtime implementation/test bounds were changed.
- Deployment protocol golden tests pass on native, Node and headless Chrome.
  Cortex-A builds pass for armv7a-none-eabi, armv7a-none-eabihf and
  aarch64-unknown-none, plus all dependency-boundary target audits.
- Actual QEMU corruption tests now pass both selection/confirmation/nonce copies,
  torn/omitted writes, corrupted readback and blocked-write power cuts. The
  consolidated run has also passed single-copy configuration/state/counter damage,
  both-copy state recovery and incompatible state versions.
- Final command-surface review added an explicit board Fastboot allowlist. Stock
  `boot`, `set_active`, alternative reboot modes, raw writes and shell commands
  are refused; verified staging/reboot still passes on real QEMU. TFTP additionally
  reserves an LMB guard before transfer, enforcing the 32 MiB limit before copying
  into the loader's reserved memory. Final suite/CI validation remains in progress.

- All deployment qualification phases passed on the restricted loader: native/UI
  boot, A/B roundtrip, five watchdog modes, all 15 storage cases, all ten metadata/
  image damage cases, interrupted/lossy networking, command/image refusals, real
  FreeRTOS cleanup refusal/retry, missing server and oversized TFTP fallback.
  The combined run is finishing the original full firmware fixture gate.
- Added the documented interactive SD emulator command. Provision → launch →
  one-command update → terminate → cold launch of the same card passed, with
  fresh nonces and the updated image. Default UDP 5554 and adjacent CLI journal
  also pass against actual QEMU.
- CodeQL's two clear-text alerts referred to a fixed public test marker; its
  naming is corrected and the audit still quarantines secret-bearing output.
- Preserve Codecov's 80% patch floor by instrumenting the workstation CLI during
  actual QEMU scenarios, with Arm builds unchanged. The measured CLI entry point
  is 82.42% and deployment transport/orchestration 88.08%; CI merges this LCOV
  report with native coverage. New successful-result/wire-tag contract tests pass
  on native/Node/Chrome. Full native suite: 2,546 passed, five intentional skips.

- The artifact audit found real authentication tokens in TCP packet captures.
  Public captures now redact all TCP application data, update checksums and use
  `.redacted.pcap`; raw originals stay private. Failure uploads accept only the
  redacted filename pattern, including if a job dies before finalization.
  Split-payload/endian/truncation tests and 64 real-capture audits pass.
- The original long fixture exposed a sampling race: its independent query task
  could observe temporary snapshot buffers. Bounded lower-watermark windows keep
  exact retained-heap equality and all C/socket/768-MiB peak checks. The complete
  fixture passed with 386,468,024 live bytes before/after 100 measured cycles,
  then passed recording-pool exhaustion, export refusal and cleanup retry.
  Production runtime behavior and resource bounds are unchanged.
- Restarting consolidated validation with the final artifact/privacy and sampling
  implementation; PR #924 remains ready for review and unmerged. Both initial
  CodeQL review threads are addressed and resolved.
- Final harness review caught a missing forwarding flag between the parent suite
  and coverage-enabled child phases. Forward `--native-coverage` explicitly;
  the actual coverage-configured records/ASan/redaction child passes. Restart the
  complete run so its report proves the CLI used throughout was instrumented.
- Final cached deployment run passed all 16 phases, including the instrumented
  CLI and the public-output audit (65 redacted captures, no quarantined files).
  CLI entry-point coverage is 83.52%; transport/orchestration is 88.08%.
  The original complete firmware suite passed all seven phases in 748.6 seconds.
- Fresh CI also passed all deployment phases and the public-output audit. Its
  following Codecov upload failed because Debian slim lacked `gpg`, required by
  the action's signature verification. Add that package; keep verification and
  the 80% patch threshold enabled. Every other applicable CI check passed.
