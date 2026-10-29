# Initial runtime performance report — archived

Current measurement observations and limits are in
[VALIDATION.md](../../VALIDATION.md#performance-observations).
Use the existing [capacity and measurement guide](../../src/rust/rt_runtime/docs/part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md)
and `scripts/benchmark_rt_runtime.py` for the real benchmark procedure.
Correctness/guards/bounds/recovery gate success, not deadline tails; keep outliers.

The full original workload/configuration/timing/tail report remains unchanged in
[offline history](../history/README.md) and
[immutable source](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/rt_runtime/PERFORMANCE.md).
Raw [evidence/](evidence/) remains in place; old samples are not new merged-head data.

<a id="reproducible-bounded-runtime-measurements"></a>
<a id="short-ci-measurement-with-the-same-correctnessguard-checks"></a>
<a id="workloads-and-checks"></a>
<a id="recorded-linux-results"></a>
<a id="tail-investigation"></a>
<a id="actual-browser-failure-detection"></a>

[Current measurement evidence](../../VALIDATION.md#performance-observations).
