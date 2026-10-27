# Chapter 3 review: borrowed decisions and required media access

Destination: `docs/part-1-process-audio/03-writing-processors.md`.
Read the complete chapter, `first_graph`/`prepared_processor` sources, current
`processor.rs`, planner contexts and relevant executor paths. Reviewed hashes and
native debug/release example checks are in `../part-one-baseline.json`.

## Preserve the working introduction

The prepared two-frame delay, variable3/1/4-frame calls, boxed dispatch and fixed
signature explanation are correct. The example guards processing, asserts exact
samples and destroys the delay outside rendering. Keep the excerpt markers and
short example-led progression. Its max-delay bound is correctly labelled example
policy, not a runtime limit. No new delay implementation is needed.

## Add focused links/explanations where the capability first appears

- At the planner-access sentence, point to chapter13: `planning()` returns prepared
  `ConsumerAccess`; explicit `DecisionBinding` validates the one decision slot a
  consumer borrows. `ProcessBlock::decision` is an immutable interval-tagged view,
  not mutable peer/resolver state. `observations` stamps the producer and enforces
  its reservation; a consumer cannot spend another consumer's unused allowance.
  Borrowed decisions/observations cannot be retained across later planner mutation.
- At media use, link chapters9/11/16. `block.resources.render()` grants required
  read/capture staging access with separate work accounting; ordinary resource
  methods use the remaining callback background allowances. Reborrowing or
  receiving another internal interval does not refresh either lifecycle. Do not
  teach playback by repeatedly charging optional export/transfer credits.
- Consumers may stage their assigned capture data but do not acquire planner
  capture lifecycle authority. A stamped planner boundary performs reservation,
  activation/commit/freeze. Put full ownership/capacity rules in chapter16 rather
  than folding the capture coordinator into this delay example.
- Include interval-relative MIDI offsets with the current interval-length advice;
  packet offsets inside a processor are not offsets into the original host block.
- Keep auxiliary resource refusals local when audio must continue. Do not blindly
  propagate a resource `Full` as a processor MIDI-overflow report. The existing
  distinction between ordinary MIDI-output recovery and fatal processor errors is
  essential; chapter8 must teach complete-packet failure and reserved host recovery.
- Keep no-heap/no-lock/no-I/O restrictions on process, parameter and typed operation
  handlers, and the explicit lease-release warning at destruction. Heap guards
  establish allocator behavior only, not arbitrary third-party nonblocking safety.

## Verification and attribution

`planned_gain` is the focused follow-up example for decisions/observations; current
planning graph and execution tests cover missing/stale/duplicate bindings, observation
reservations, exact intervals and callback-wide allowances. Runtime view/capture
lifetime prototypes independently reject retained authority. Loop renderers,
record-to-play, restoration and composite timing belong to `shoop_loop_engine`, not
the generic runtime. This review does not claim new browser/device execution or
full later-chapter examples; those remain separately required.
