# Composite RT manual-validation package

## Status and ownership

All scenarios are **pending user execution**. This document is the handoff package, not evidence that live behavior passed. Record results in the fields below and attach captured sessions/logs when reporting a failure.

A finding that violates feature parity, RT authority, sample correctness, bounded overload, or callback safety reopens the relevant implementation stage.

## Required setups

Record at least one representative setup for each applicable backend.

| Field | JACK setup | CPAL setup |
|---|---|---|
| Date/time | Pending | Pending |
| Commit | Pending | Pending |
| OS/kernel | Pending | Pending |
| Backend/device | JACK | Pending |
| Sample rate | 48 kHz recommended | 48 kHz recommended |
| Buffer size | 64 frames, then lowest stable value | 64–256 frames or lowest supported |
| MIDI controller | Optional but recommended | Optional but recommended |
| Input source | Microphone/instrument or deterministic playback | Same |
| Result owner | User | User |

Before testing:

1. Build and run the normal top-level application, not an engine-only harness.
2. Confirm the application reports the intended backend, sample rate, and callback size.
3. Reset xrun and composite fault/overflow counters.
4. Enable or capture composite transition diagnostics, including sample timestamp, stable target identity, winning action/origin, conflict count, timeline version, snapshot drops, stale targets, queue rejection, RT fault, sub-block overflow, and xruns.
5. Keep a screen/audio recording where practical. Use a click/transient source for boundary checks.

## Required session packages

Create and save these sessions before the stress/edit variants, then use the saved copies for reload checks:

- `composite_rt_sequential_parallel`: one sync loop, a regular sequential composite with a repeated child and delay, and a parallel timeline with a different-length child.
- `composite_rt_record_grab`: empty child loops routed to representative live inputs; play-after-record is exposed and ringbuffers are long enough for the full composite.
- `composite_rt_nested_scripts`: at least three levels covering regular→regular, regular→script, script→regular, and script→script across the saved session.
- `composite_rt_capacity`: the largest supported target/action schedule documented by the application without exceeding accepted capacities.

Record actual saved paths and hashes:

| Session | Path | Hash/version | Loads successfully |
|---|---|---|---|
| Sequential/parallel | Pending | Pending | Pending |
| Record/grab | Pending | Pending | Pending |
| Nested/scripts | Pending | Pending | Pending |
| Capacity | Pending | Pending | Pending |

## Common result record

For every scenario record:

- **Result:** Pending / Pass / Fail / Blocked / Not applicable.
- **Setup/backend:**
- **Session path/version:**
- **Start/end time:**
- **Expected versus observed:**
- **First bad sample or musical boundary:**
- **Xruns before/after:**
- **Composite counters/fault before/after:**
- **Timeline version and trace excerpt:**
- **Audio/video/log attachments:**
- **Exact reproduction steps:**

## Scenarios

### M01 — Low-buffer sequential and parallel playback

1. Load `composite_rt_sequential_parallel` under JACK at 64 frames.
2. Start the regular composite for at least 32 complete passes.
3. Repeat at the lowest stable callback size.

Expected: sequential, delayed, repeated, and parallel children change at the intended boundary with no extra idle sample, missed/multiple iteration, xrun attributable to the composite, or unexplained fault. Repeated contiguous playback does not retrigger/glitch.

**Result record:** Pending.

### M02 — Representative CPAL playback

Run M01 using the representative CPAL device and its fixed/selected callback size.

Expected: the same transition order and musical boundaries as JACK. Device scheduling may change command acceptance latency, but an accepted configuration does not change timing behavior.

**Result record:** Pending.

### M03 — Start, stop, cancel, and retrigger near boundaries

Issue immediate and synchronized starts/stops on both sides of a visible sync boundary. Arm countdowns of 0, 1, and several boundaries; replace and cancel pending transitions.

Expected: displayed countdown skips exactly the requested boundaries; stop suppresses a coincident due source action; child cleanup is complete; no command is silently applied one boundary late.

**Result record:** Pending.

### M04 — Composite recording and play-after-record

Record the regular composite once with play-after-record enabled and once disabled. Include a repeated child.

Expected: each target records only its first scheduled occurrence. Enabled begins playback at iteration zero at pass end; disabled stops composite and children. Changing the toggle before versus after RT acceptance has the documented cutoff behavior.

**Result record:** Pending.

### M05 — Nested regular/script scenes

Load `composite_rt_nested_scripts`, execute every parent/child kind combination, and let regular parents cycle for at least 32 passes.

Expected: nested iteration-zero actions reach primitive children at the parent's sample; scripts complete once; regular plans cycle; no Qt/update-thread recursion, extra sync delay, ordering variation, or cycle fault appears.

**Result record:** Pending.

### M06 — Immediate synchronization/seek

While audio runs, seek regular and recording composites to first, middle, last, and a changed iteration. Attempt one invalid iteration through a control surface that exposes validation.

Expected: active children, offsets, composite iteration/position, and next action are immediately coherent. Invalid seek reports rejection and leaves state unchanged; no schedule replay stall is visible.

**Result record:** Pending.

### M07 — Composite ringbuffer grab matrix

Using `composite_rt_record_grab`, execute:

- synchronized default-length grab then stop;
- synchronized fixed-length grab then stop;
- synchronized grab then play;
- unsynchronized grab then stop;
- unsynchronized grab then play;
- empty/undefined sync-source rejection.

Expected: first recording ranges map to the intended input windows, all children commit as one documented transaction/boundary sequence, lengths/positions match the selected cycle, and no intermediate child state leaks to audio.

**Result record:** Pending.

### M08 — Configuration edits in stopped, pending, and running states

Edit delay, repeat, explicit duration, child mode, parallel structure, and child length. Test while stopped, while a countdown is pending, and while running.

Expected: stopped/pending activation and running activation follow `SEMANTICS.md`; version errors are visible; no old/new partial schedule, runtime reset, stale child action, or GUI-timed transition occurs.

**Result record:** Pending.

### M09 — GUI and frontend/update-thread stall

Start a configured composite, then separately:

1. freeze/heavily load the GUI thread;
2. freeze/heavily load frontend update processing;
3. perform blocking file I/O for several seconds.

Expected: audio-thread transition trace and audible sequence continue at exact boundaries. Observation may lag/drop, then catches up from snapshots/rolling trace without becoming timing input.

**Result record:** Pending.

### M10 — Keyboard/MIDI command latency

Trigger start, stop, retrigger, and seek from keyboard and MIDI just before/after quantization boundaries.

Expected: perceived eligibility matches the documented callback acceptance boundary. GUI scheduling may defer acceptance, but accepted commands are never moved to a later musical boundary by frontend delivery.

**Result record:** Pending.

### M11 — Save/load compatibility and lifecycle

Load existing composite sessions, execute them, save under a new name, close/replace the session, and reload. Include nested regular/scripts and conversion between empty basic and composite slots.

Expected: `loop.1` playlists/modes/durations remain compatible; references resolve only to the intended stable identities; teardown sends no stale event and does not destroy RT-owned allocations on the callback.

**Result record:** Pending.

### M12 — Large supported schedule and overload diagnostics

Run `composite_rt_capacity` for at least ten minutes while recording xruns, callback cost, snapshot drops, conflict/stale counters, and RT faults. Separately attempt one configuration above each exposed plan/queue capacity.

Expected: supported load remains within the documented callback budget. Above-capacity plans/commands reject explicitly before acceptance. Runtime overload latches fail-closed at the exact boundary and never becomes a late event.

**Result record:** Pending.

### M13 — Boundary quality and unexplained-delay review

Review recordings/traces from M01–M12 for audible clicks, one-sample gaps, extra sync-cycle delay, unstable same-sample winner, or trace/audio disagreement.

Expected: none. Every audible anomaly maps to an explicit xrun/fault/overload record or is filed with the first bad sample and reproduction package.

**Result record:** Pending.

## Final user sign-off

- All applicable scenarios passed: Pending.
- Blocked/not-applicable scenarios justified: Pending.
- No immutable requirement reopened: Pending.
- User name/date: Pending.
- Attachments/index: Pending.
