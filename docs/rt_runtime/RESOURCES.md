# Runtime resource contract — consolidated

See [ARCHITECTURE.md — resources/capture/media](../../ARCHITECTURE.md#resources-capture-and-media)
and the [runtime media chapters](../../src/rust/rt_runtime/docs/part-3-work-with-stored-media/09-loading-and-reading-resources.md)
for current contracts/APIs. Current evidence is in [VALIDATION.md](../../VALIDATION.md).
Prepared promises, immutable leases, refusal and incremental reclamation remain
explicit; no unlimited capture or callback allocation is implied.

Full original detailed contract and historical Stage 5 requirements:
[offline archive](../history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/rt_runtime/RESOURCES.md).

<a id="resource-implementation-contract"></a>
<a id="ownership-and-fixed-capacities"></a>
<a id="formats"></a>
<a id="upload-state-machine"></a>
<a id="capture-and-immutable-prefix-snapshots"></a>
<a id="reading-and-playback"></a>
<a id="prepared-all-member-capture"></a>
<a id="reclamation-and-work-accounting"></a>
<a id="initial-stage-5-evidence-requirements-verified-in-validationmd"></a>

[Continue in the unified architecture](../../ARCHITECTURE.md#resources-capture-and-media).
