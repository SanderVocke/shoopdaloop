# Chapter 12: reconstruct strict pre-target protocol state

Destination currently empty. Reviewed `midi_state.rs`, MIDI pool/read/parser APIs,
archive state/index guides and relevant tests; hashes/checks are recorded in
`../media-baseline.json`. This is a required example outline, not a claim that a
new chapter/example has been authored or validated.

## Small checkpoint example to implement

Prepare `ResourceStore::with_midi_indexing(config, MidiIndexConfig { versions,
checkpoints_per_version: 3, replay_events: 2 })` with enough versions and leases for
retention. The per-version event capacity here is **5**, not6: checkpoint zero stores
initial state and another checkpoint follows each complete pair of events.

A compact independent reference can use an initial held note62/velocity40 and:

| Relative frame | Original packet |
| --- | --- |
| 0 | note-on60/100 |
| 1 | sustain64=127 |
| 1 | note-off60/45 |
| 3 | sustain64=0 |
| 4 | note-on61/80 |

Set duration6. Use `begin_indexed_midi_upload` with the supplied initial state,
stream/finish/commit and pin it. The initial note62 is source history before the
recording, not a synthetic packet inserted at frame0.

- Seek1: state contains the held note60 from0, but neither packet at1 has applied;
  the cursor retains sustain then note-off in their original order.
- Seek2 or3: note60 is released but retained by sustain. At3 the cursor still has
  the original pedal-up packet; state is strictly before it.
- Seek4: note60 is gone after pedal release, note62 remains held, and note61's
  original packet has not yet applied.
- Seek6: EOF with the final state including note61; beyond6 is InvalidFrame.
- Every result reports `replayed_events < 2`. Compare the whole relevant projection
  against an independently assembled state/packet reference, not a second call to
  the same seek function. Preserve original payloads for later playback.

Add explicit malformed-chunk and capacity-refusal branches: a chunk containing an
otherwise valid earlier packet followed by malformed data publishes neither;
validation work remains spent. A sixth record exceeds this declared checkpoint
capacity even if byte pages remain. Keep any unfinished upload cancellable and
recover all ownership. For capture, checkpoint exhaustion preserves the preceding
complete prefix with an incomplete reason, not a partial event or resumed gap.
A separate empty/legacy case must demonstrate initial-state EOF and explicit refusal
of `midi_seek` without indexed metadata. Complete capture coordination is chapter16.

## Explain the actual bound and ownership

For C checkpoints and span K, at most `C × K −1` packets fit. All complete packets
count, including opaque SysEx/realtime messages. A version reserves an entire row
plus a mutable state slot; approximate protocol-state storage is
`versions × (1 + C) × size_of::<MidiState>()` plus metadata/free stacks. Arithmetic
is checked; this is not a promise of available OS memory or callback deadlines.

Seek selects the last checkpoint whose final packet is strictly before the target
and within the pinned prefix, then replays fewer than K records. It inspects indexed
headers/short state data, not whole-history or huge SysEx-body scans. Checkpoint
search is logarithmic; page-position cost is additional. A broken replay invariant
is InvalidState, not permission to drop/delay musical work. Freeze builds no index.
Old leased revisions retain rows; final release recycles row identity/used length
without clearing all checkpoint state, while media-page reclamation remains bounded.
Required reads/reconstruction do not spend background allowance. Diagnostic record,
replay and conceptual state-copy counters saturate and reset once per callback.

## Protocol policy and renderer boundary

Explain fixed16-channel/128-key projection, held versus sustain/hold/sostenuto keys,
note-on-zero, repeated note-ons, pressure/pitch, channel-mode/reset behavior and
unknown controls. Retain the loaded-program bank separately from pending selectors;
keep last RPN/NRPN family and the explicit untracked parameter-history marker.
CC96/97 and momentary reset/all-notes commands must not become cached controls to
replay after note restoration. Do not invent device-specific reset defaults.

`MidiState::apply_packet` validates atomically. `from_channels` only reassembles
copies: channels/reset marker must come from one immutable cut, not independently
sampled live fields. System Reset is marked; arbitrary SysEx/synth voice/envelope
and parameter histories are not reconstructed.

State lookup emits nothing. Cleanup→restoration→equal-time originals is renderer
work. Attribute the implemented helpers/indexed playback/capture to
`shoop_loop_engine`, with sink failure/recovery and explicit unsupported-state
reporting, not to this generic value/index API. The archive's old blanket “no loop
integration” wording must not overwrite current Stage5/6 implementation evidence;
actual independent loop AudioWorklet evidence is still a separate pending stage.

Existing `midi_state`, `midi_restore_state` and all eight `midi_index` tests ran in
native debug/release within the63-case review. Keep their independent/property
coverage distinct from the future small teaching example and from Miri status.
