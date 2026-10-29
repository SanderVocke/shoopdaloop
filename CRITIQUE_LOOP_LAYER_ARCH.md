# Loop composability critique — consolidated

See [ARCHITECTURE.md — open composability decision](ARCHITECTURE.md#open-composability-decision).
The current facade owns the runtime; a reusable subsystem plus optional facade is
a **proposed** alternative, not an implemented API or an approved refactor.
All timing, binding, capture/publication and ownership guarantees must survive it.

Full analysis: [offline archive](docs/history/README.md) ·
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/CRITIQUE_LOOP_LAYER_ARCH.md).

<a id="loop-layer-architecture-scope-and-composability-critique"></a>
<a id="summary"></a>
<a id="current-scope"></a>
<a id="timing-topology-is-not-the-signal-graph"></a>
<a id="runtime-ownership-versus-routing-policy"></a>
<a id="what-invariants-does-the-integration-make-easier-to-enforce"></a>
<a id="1-required-consumers-and-decision-bindings-remain-present"></a>
<a id="2-a-program-and-its-associated-routing-publish-together"></a>
<a id="3-reassigning-physical-roles-cannot-leave-mixed-logical-identities"></a>
<a id="4-capture-and-musical-state-agree-before-affected-samples-render"></a>
<a id="architectural-assessment"></a>
<a id="preferred-boundary-for-a-composable-loop-layer"></a>

[Continue in the unified architecture](ARCHITECTURE.md#open-composability-decision).
