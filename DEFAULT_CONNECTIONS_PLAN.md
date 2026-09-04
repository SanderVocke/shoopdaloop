# Unified default output connections implementation plan

## Feasibility and goal

This is feasible with the existing explicit mixer-route and host-connection models. The policy
belongs in the application/session-creation layer, while backend-specific playback-target
discovery belongs behind the backend abstraction. The audio engine should continue to contain no
implicit routing.

The goal is to make output routing consistent across native and browser runtimes:

- A new track may request a destination bus by display name. The shipped preference is `Master`.
- At track creation time, route its audio outputs to that bus when an eligible bus exists.
- Equal channel counts map one-to-one. A mono track fans its only output out to every channel of an
  N-channel bus. All other unequal shapes remain unconnected.
- A new session's default Master bus connects once to the first suitable system playback target.
- No new track connects directly to a system output by default. Manual direct routes remain valid.
- Input auto-connection behavior remains unchanged.

## Immutable acceptance criteria

1. Track defaults contain a string-valued output-bus preference whose factory value is `Master`;
   an empty value means no automatic bus route.
2. The Add Track dialog shows and edits the same value, initializes it from settings, and includes
   it in the existing **make default** save flow.
3. Bus lookup occurs only while creating the track. Later bus creation or renaming does not
   retroactively route the track.
4. Lookup is deterministic: inspect buses in visible session order and select the first bus with
   the requested exact, case-sensitive display name and a compatible channel count.
5. For track and bus channel counts `T` and `B`, create routes `track[i] -> bus[i]` when `T == B`,
   and `track[0] -> bus[0..B]` when `T == 1`. Do nothing for every other shape, for zero audio
   outputs, or when no eligible named bus exists.
6. The route uses the track's audible audio outputs: direct outputs for regular tracks and wet
   outputs for processed dry/wet tracks. MIDI and dry/sends are not routed by this preference.
7. Creating a fresh session (including the initial untitled session) creates the default stereo
   Master and makes its two outputs target one ordered two-channel system playback group, once
   suitable endpoints are discoverable.
8. CPAL chooses the active output device's playback channels, WebAudio chooses the destination
   channels, and JACK prefers an ordered physical audio sink group reported by JACK. If no complete
   two-channel group is available, Master remains unconnected and the condition is reported without
   inventing a partial route.
9. Loading an existing session restores exactly its persisted mixer and external routes and does
   not apply either new-session bootstrap policy. Driver replacement preserves desired explicit
   routes under the existing replacement rules; it does not re-run track creation policy.
10. New tracks have no automatic direct output-to-host connections on any backend. Existing saved
    direct routes and user-created direct routes continue to work. Existing input defaults are
    unchanged.

## Design rules and constraints

- Keep every effective connection explicit and serializable. Defaults create ordinary mixer or
  host links; neither the engine graph nor playback processing may synthesize hidden links.
- Treat the bus name as creation-time user intent, not persistent track identity. Do not add it to
  the session document: the concrete routes produced from it are already persisted.
- Add the preference to the application-facing `TrackSpec` (for example
  `initial_output_bus_name: Option<String>`), not to low-level `TrackRequest`. The application owns
  bus display order/names and translates application port/channel IDs to backend IDs.
- Centralize initial mixer-route construction in one application helper used by every runtime.
  Do not duplicate policy in native, Worker, AudioWorklet, or browser adapters.
- Preserve duplicate bus names as valid. “First” means current session/model order, which is stable
  and visible to the user.
- Model playback discovery as ordered channel groups rather than guessing from flattened port
  names in the UI. Extend the backend snapshot/API with a small playback-target descriptor if
  necessary. For JACK, retain the JACK metadata needed for ranking (physical/terminal flags,
  client, aliases, and JACK enumeration order); prefer a physical terminal sink group, group by
  client/device, natural-sort channel-like port names, and use the first complete group. CPAL and
  WebAudio publish their known device/destination channels as their preferred group.
- New-session Master bootstrap is a one-shot desired operation that may wait for asynchronous host
  discovery. Mark it consumed after success or after discovery has authoritatively completed with
  no suitable group; never re-run it merely because a user disconnects Master.
- Remove only WebAudio **output** auto-connect code. Keep its capture/input auto-connect path intact.
- Use existing pending/error connection semantics so the Connections UI remains authoritative.

## Staged implementation

### Stage 0 — branch and baseline

- [x] Create a focused feature branch from the intended current base and record the baseline SHA.
- [x] Run focused existing settings, track-creation, mixer-route, new-session, CPAL, WebAudio, and
      JACK discovery tests before changing behavior.
- [x] Verification: document baseline pass/fail/skip counts and distinguish unavailable real audio
      facilities from product failures.

### Stage 1 — preference and Add Track contract

- [x] Register a `tracks.new.default_output_bus` string setting with factory value `Master`,
      `NextUse` effect, and Track defaults presentation.
- [x] Add the value to `NewTrackConfiguration`, its settings draft read/write paths, dialog state,
      shared configuration grid, and **make default** rebasing/save flow.
- [x] Extend `TrackSpec` with normalized optional creation-time bus intent and update constructors,
      script/test fixtures, and validation as required.
- [x] Verification: unit/UI tests cover factory default, empty opt-out, dialog initialization,
      editing, settings-window editing, and successful/failed make-default persistence.
- [x] Commit the completed preference/UI/API stage.

### Stage 2 — creation-time track-to-bus routing

- [x] After a track's application output ports are registered, resolve the first compatible named
      bus and construct the deterministic one-to-one or mono fan-out route set.
- [x] Submit those routes through the same backend and pending/confirmation machinery used by
      `SetMixerRouteConnected`; make partial submission impossible or roll it back before reporting
      track creation success.
- [x] Ensure dry/wet topology selects only wet audible outputs and trigger/zero-audio tracks are a
      no-op.
- [x] Verification: application/backend tests cover mono-to-mono, mono-to-stereo/N, stereo-to-stereo,
      equal N-to-N, mismatches, missing names, duplicate names/order, dry/wet role selection, backend
      rejection, route persistence, and no late routing after a bus appears or is renamed.
- [x] Commit the completed creation-policy stage.

### Stage 3 — preferred playback-target discovery

- [x] Introduce an ordered playback-target/group descriptor at the backend boundary, distinct from
      the flattened host-port list used by the connection dialog.
- [x] Populate it directly from active CPAL output channels and WebAudio destination channels.
- [x] Enhance JACK discovery to query JACK port flags/metadata, exclude the application's own ports,
      retain only audio input sinks, rank physical terminal groups ahead of other clients, and return
      deterministic ordered channels.
- [x] Verification: pure ranking tests cover common JACK names, multiple devices/clients, monitors,
      non-physical sinks, insufficient channel groups, and deterministic ties; backend tests cover
      CPAL mock and WebAudio groups; retain an optional real-JACK integration smoke test.
- [x] Commit the completed discovery stage.

### Stage 4 — new-session Master bootstrap

- [x] Tag fresh-session creation separately from ordinary session load/replacement and arm a
      one-shot Master-output bootstrap only for that path (also for initial startup).
- [x] Once both Master ports and a complete preferred playback group are known, issue ordinary
      `SetPortConnected` operations pairing channels by order.
- [x] Persist the resulting explicit Master host links normally. Do not bootstrap loaded sessions,
      legacy migrations, or driver switches, and do not reconnect after a manual disconnect.
- [x] Verification: tests cover initial startup, New Session, delayed discovery, no target,
      incomplete target, connection rejection, save/reload, manual disconnect, loaded empty routes,
      and driver replacement.
- [x] Commit the completed new-session stage.

### Stage 5 — remove direct output defaults without changing inputs

- [x] Delete WebAudio's per-track destination auto-connect branch while retaining WebAudio capture
      auto-connect behavior exactly as-is.
- [x] Audit native, CPAL, JACK, Worker, and AudioWorklet creation paths to ensure no other direct
      output defaults remain or bypass the application policy.
- [x] Update legacy browser migration deliberately: preserve already-persisted direct links, but do
      not manufacture new direct output routes for fresh/current documents. If legacy compatibility
      must remain, constrain it to the existing explicit legacy marker and document that exception.
- [x] Verification: cross-runtime tests assert zero automatic direct track-output links, default
      track-to-Master links, default Master-to-device links for fresh sessions, manual direct routing,
      and unchanged input capture defaults.
- [x] Commit the completed policy-unification stage.

### Stage 6 — end-to-end validation and delivery

- [x] Run Rust formatting, warning-denying builds, relevant policy scripts, the complete native test
      suite, all Wasm/Node packages, locked Wasm app/worklet builds, and browser smoke tests.
- [x] Run a native CPAL smoke, browser smoke, and JACK smoke where devices are available; inspect the
      Connections UI and verify the saved session contains only the explicit links shown there.
- [x] For the perceptible UI change, capture screenshots of Track defaults and Add Track showing the
      new control.
- [ ] Push the branch, open a PR describing behavior, migrations, backend target-selection rules,
      and exact validation results, and drive CI green.
- [ ] Check automated review repeatedly; address valid feedback with tests and commits until review
      approves and required CI remains green.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
