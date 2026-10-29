# Loop facade design — consolidated

Use [ARCHITECTURE.md](../../ARCHITECTURE.md#resources-capture-and-media) and its
[publication/cleanup contract](../../ARCHITECTURE.md#prepared-publication-and-teardown).
The current facade remains graph-owning; the proposed alternative is explicitly
an [open decision](../../ARCHITECTURE.md#open-composability-decision), not a delivered API.
Public usage remains in the [loop guide](../../src/rust/shoop_loop_engine/docs/runtime.md).

Full original rationale and scoped verification:
[offline archive](../history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/FACADE_DESIGN.md).

<a id="coordinated-loop-facade--implementation-design"></a>
<a id="ownership-and-preparation"></a>
<a id="stable-boundary-execution"></a>
<a id="receipts-observations-and-coherent-export"></a>
<a id="graph-and-lifecycle-integration"></a>
<a id="verification"></a>

[Continue in the unified architecture](../../ARCHITECTURE.md).
