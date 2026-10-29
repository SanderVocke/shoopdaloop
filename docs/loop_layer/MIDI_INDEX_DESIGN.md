# MIDI indexing design — consolidated

Current fixed-storage/checkpoint/strict-pretarget contracts are in
[ARCHITECTURE.md](../../ARCHITECTURE.md#resource-ownership-and-indexed-access).
Use the existing [MIDI access chapter](../../src/rust/rt_runtime/docs/part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md)
and [checkpoint chapter](../../src/rust/rt_runtime/docs/part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md)
for APIs. Index evidence does not by itself prove restoration or a full host.

Full original: [offline archive](../history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/MIDI_INDEX_DESIGN.md).

<a id="midi-eventcheckpoint-integration-design"></a>
<a id="prepared-storage"></a>
<a id="construction-and-refusal"></a>
<a id="positioning-and-packet-access"></a>
<a id="historical-verification-scope"></a>

[Continue in the unified architecture](../../ARCHITECTURE.md#resource-ownership-and-indexed-access).
