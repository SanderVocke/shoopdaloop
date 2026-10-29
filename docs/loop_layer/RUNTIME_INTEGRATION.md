# Resolver/runtime integration contract — consolidated

Current contract: [ARCHITECTURE.md](../../ARCHITECTURE.md#one-callback-envelope-and-borrowed-authority),
[typed delivery](../../ARCHITECTURE.md#admission-scheduling-cancellation-and-results),
[capture](../../ARCHITECTURE.md#coordinated-capture) and
[publication](../../ARCHITECTURE.md#prepared-publication-and-teardown).
Use the existing runtime book/loop guides for public APIs, and
[VALIDATION.md](../../VALIDATION.md) for actual evidence.

Full original failure table, API/lifetime design and prototype history:
[offline archive](../history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/RUNTIME_INTEGRATION.md).

<a id="resolverruntime-integration-contract"></a>
<a id="status-and-dependency-boundary"></a>
<a id="generic-api-shape-and-lifetimes"></a>
<a id="planner-identity-and-graph-descriptors"></a>
<a id="callback-versus-interval-lifecycle"></a>
<a id="typed-delivery-preparation-and-owning-returns"></a>
<a id="resource-access-and-group-capture"></a>
<a id="work-classes"></a>
<a id="indexed-access-and-recording-readiness"></a>
<a id="failureobservation-table"></a>
<a id="state-publication-and-browser-integration"></a>
<a id="historical-prototype-verification-and-limitations"></a>
<a id="expected-nonzero-exit-with-e0502-not-a-missing-filecompiler-error"></a>

[Continue in the unified architecture](../../ARCHITECTURE.md).
