# Modular transport proposal — consolidated

See [ARCHITECTURE.md — proposed modular protocol](ARCHITECTURE.md#proposed-modular-protocol)
and [PLAN.md — proposed next work](PLAN.md#proposed-next-work). The common framework
and service browser/IPC/network deployments remain **proposed**, not delivered
by existing local transports, test-host codecs or Wasm library compilation.

Full proposal, deployment/lifecycle details and unresolved questions:
[offline archive](docs/history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/PROPOSAL_TRANSPORT.md).

<a id="proposal-modular-transport-independent-engine-communication"></a>
<a id="status-and-scope"></a>
<a id="motivation"></a>
<a id="existing-mechanisms-and-their-limits"></a>
<a id="local-runtime-transport"></a>
<a id="loop-layer-and-current-wire-examples"></a>
<a id="architectural-responsibilities"></a>
<a id="1-common-protocol-infrastructure"></a>
<a id="2-domain-schemas"></a>
<a id="3-domain-adapters"></a>
<a id="4-application-composition"></a>
<a id="5-carrier-adapters"></a>
<a id="example-an-independently-supplied-audio-plugin-host"></a>
<a id="message-and-identity-model"></a>
<a id="execution-and-result-semantics"></a>
<a id="flow-control-ownership-and-rt-safety"></a>
<a id="reliable-work-and-credits"></a>
<a id="traffic-classes"></a>
<a id="decoding-and-prepared-owners"></a>
<a id="deployment-modes"></a>
<a id="native-in-process"></a>
<a id="browser-independent-wasm-instances"></a>
<a id="native-cross-process-ipc"></a>
<a id="cross-machine-socketsnetwork"></a>
<a id="bulk-media-and-component-state"></a>
<a id="telemetry"></a>
<a id="lifecycle-reconnects-and-security"></a>
<a id="cross-domain-coordination-and-local-api-composition"></a>
<a id="validation-and-unresolved-decisions"></a>

[Continue in the unified architecture](ARCHITECTURE.md#proposed-modular-protocol).
