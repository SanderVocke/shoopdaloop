# Runtime proposal — archived

Current design: [ARCHITECTURE.md](ARCHITECTURE.md#scope-and-implementation-map).
The initial runtime is delivered; loop/service implementation and open acceptance
are distinguished in [PLAN.md](PLAN.md) and [VALIDATION.md](VALIDATION.md).
This original proposal is history, not a claim that current hosts are absent or
that production migration has happened.

Full original: [offline archive](docs/history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/PROPOSAL_RT_ENGINE.md).

<a id="proposal-a-generic-real-time-graph-runtime-crate"></a>
<a id="status-and-implemented-next-layer"></a>
<a id="feasibility-verdict"></a>
<a id="why-direct-extraction-is-unsuitable"></a>
<a id="original-recommended-architecture"></a>
<a id="1-real-time-core"></a>
<a id="2-control-and-resource-layer"></a>
<a id="3-host-adapters"></a>
<a id="command-model"></a>
<a id="safe-buffer-fetching-and-replacement"></a>
<a id="replacement"></a>
<a id="fetching"></a>
<a id="browser-limitation"></a>
<a id="original-extraction-boundary"></a>
<a id="what-should-remain-in-shoop"></a>
<a id="reducing-nativebrowser-duplication"></a>
<a id="original-optional-migration-roadmap"></a>
<a id="1-characterize-current-behavior"></a>
<a id="2-create-an-internal-generic-runtime-crate"></a>
<a id="3-put-the-existing-session-behind-the-runtime"></a>
<a id="4-create-the-shared-shoopengineservice"></a>
<a id="5-generalize-bulk-resources"></a>
<a id="6-extract-prepared-graph-plans"></a>
<a id="7-stabilize-and-enforce-the-crate-boundary"></a>
<a id="original-suggested-service-migration-criteria"></a>
<a id="conclusion"></a>

[Continue in the unified architecture](ARCHITECTURE.md).
