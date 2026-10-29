# Runtime design — consolidated

Use [ARCHITECTURE.md](ARCHITECTURE.md#accepted-architectural-constraints) for current
runtime/loop/service boundaries and contracts, the
[runtime book](src/rust/rt_runtime/docs/README.md) for public API usage, and
[VALIDATION.md](VALIDATION.md) for actual source-scoped evidence.

Full original design and evolution history: [offline archive](docs/history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/DESIGN_RT_ENGINE.md).
Historical quota/snapshot statements are not another current design authority.

<a id="generic-real-time-runtime-initial-design-and-coordination-evolution"></a>
<a id="current-implementation-map"></a>
<a id="initial-purpose-and-boundary"></a>
<a id="problems-addressed"></a>
<a id="architecture"></a>
<a id="processing-contract"></a>
<a id="graphs-and-processor-lifetime"></a>
<a id="control-timing-and-overload"></a>
<a id="resources-and-snapshots"></a>
<a id="native-and-browser-execution"></a>
<a id="initial-implementation-decisions-and-subsequent-resource-additions"></a>
<a id="initial-delivery-and-exclusions"></a>
<a id="implemented-evolution-resolver-driven-interval-execution"></a>
<a id="responsibility-boundary"></a>
<a id="executor-lifecycle"></a>
<a id="dependencies-and-signal-structure"></a>
<a id="work-storage-and-termination"></a>
<a id="additional-integration-requirements-and-verification"></a>
<a id="stage-1-integration-decisions-and-stage-3-generic-implementation"></a>
<a id="stage-6-validated-graph-banks-and-activation-permits"></a>

[Continue in the unified architecture](ARCHITECTURE.md).
