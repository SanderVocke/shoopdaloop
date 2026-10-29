# Capture transaction design — consolidated

Use [ARCHITECTURE.md — coordinated capture](../../ARCHITECTURE.md#coordinated-capture)
for claims, lifecycle/final-lease promises, prepared-first-interval staging and
planner authority. The [capture chapter](../../src/rust/rt_runtime/docs/part-4-coordinate-processors-and-changes/16-recording-resources-together.md)
is the public API path. Loop timing/facade acceptance remains separately scoped.

Full original lifecycle/copy/refusal design and evidence:
[offline archive](../history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/CAPTURE_TRANSACTIONS_DESIGN.md).

<a id="prepared-capture-groups-and-lifecycle-transactions"></a>
<a id="representation-and-ownership"></a>
<a id="lifecycle-promises"></a>
<a id="interval-reservation-and-staging"></a>
<a id="plannerconsumer-boundary"></a>
<a id="historical-evidence-scope"></a>

[Continue in the unified architecture](../../ARCHITECTURE.md#coordinated-capture).
