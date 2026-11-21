# Benchmark host runtime

Complete. Wall-clock seconds; parallel child durations overlap. Rust phase durations are inclusive and nested.

| Suite phase | Seconds |
| --- | ---: |
| setup | 0.149 |
| build | 0.338 |
| serial_host_cases | 12.449 |
| parallel_rtl_cases | 2376.700 |
| report | 1.336 |
| orchestration | 0.024 |
| Total elapsed | 2390.996 |

| Backend / media / pattern | Status | Callbacks | Child wall s | Prepare s | Execution loop s | Observation s | Finalize s | Teardown s | Timed fabric s |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cpu / audio / lifecycle | completed | 298 | 1.502 | 1.125 | — | — | 0.000 | 0.006 | — |
| cpu / audio / mixed | completed | 232 | 0.771 | 0.484 | — | — | 0.000 | 0.006 | — |
| cpu / audio / grab | completed | 5153 | 6.430 | 0.666 | — | — | 0.000 | 0.010 | — |
| cpu / midi / lifecycle | unsupported | 0 | 0.522 | 0.513 | — | — | 0.000 | 0.005 | — |
| cpu / midi / mixed | unsupported | 0 | 0.525 | 0.516 | — | — | 0.000 | 0.006 | — |
| cpu / midi / grab | unsupported | 0 | 0.534 | 0.525 | — | — | — | 0.006 | — |
| cpu / audio-midi / lifecycle | unsupported | 0 | 0.547 | 0.536 | — | — | 0.000 | 0.008 | — |
| cpu / audio-midi / mixed | unsupported | 0 | 0.539 | 0.529 | — | — | 0.000 | 0.006 | — |
| cpu / audio-midi / grab | unsupported | 0 | 0.518 | 0.509 | — | — | — | 0.007 | — |
| fpga-reference / audio / lifecycle | completed | 298 | 0.037 | 0.001 | — | — | 0.000 | 0.000 | — |
| fpga-reference / audio / mixed | completed | 232 | 0.029 | 0.001 | — | — | 0.000 | 0.000 | — |
| fpga-reference / audio / grab | unsupported | 0 | 0.003 | 0.001 | — | — | — | 0.000 | — |
| fpga-reference / midi / lifecycle | completed | 298 | 0.035 | 0.001 | — | — | 0.000 | 0.000 | — |
| fpga-reference / midi / mixed | completed | 232 | 0.025 | 0.001 | — | — | 0.000 | 0.000 | — |
| fpga-reference / midi / grab | unsupported | 0 | 0.003 | 0.001 | — | — | — | 0.000 | — |
| fpga-reference / audio-midi / lifecycle | completed | 298 | 0.052 | 0.001 | — | — | 0.000 | 0.000 | — |
| fpga-reference / audio-midi / mixed | completed | 232 | 0.037 | 0.001 | — | — | 0.000 | 0.000 | — |
| fpga-reference / audio-midi / grab | unsupported | 0 | 0.003 | 0.001 | — | — | — | 0.000 | — |
| fpga-rtl / audio / lifecycle | completed | 298 | 233.544 | 1.289 | 230.739 | 0.220 | 0.054 | 0.024 | 0.103126 |
| fpga-rtl / audio / mixed | completed | 232 | 181.828 | 1.281 | 179.290 | 0.168 | 0.063 | 0.016 | 0.080042 |
| fpga-rtl / audio / grab | unsupported | 0 | 1.391 | 1.254 | — | — | — | 0.001 | 0.000000 |
| fpga-rtl / midi / lifecycle | completed | 298 | 1536.746 | 0.903 | 1532.702 | 0.188 | 1.356 | 0.806 | 0.710416 |
| fpga-rtl / midi / mixed | completed | 232 | 1007.703 | 0.962 | 1004.562 | 0.149 | 1.004 | 0.555 | 0.470703 |
| fpga-rtl / midi / grab | unsupported | 0 | 1.080 | 0.900 | — | — | — | 0.001 | 0.000000 |
| fpga-rtl / audio-midi / lifecycle | completed | 298 | 1751.023 | 1.133 | 1745.404 | 0.208 | 1.836 | 0.931 | 0.800670 |
| fpga-rtl / audio-midi / mixed | completed | 232 | 1187.085 | 0.996 | 1183.248 | 0.160 | 0.861 | 0.449 | 0.541757 |
| fpga-rtl / audio-midi / grab | unsupported | 0 | 1.153 | 1.029 | — | — | — | 0.001 | 0.000000 |

Full phase counters, child start/end offsets, processing timers and sampling-profile paths are retained in runtime.json.
Execution loop includes the generated RTL, signal access, agents and memory models; sampled CPU stacks distinguish those costs.
Timed fabric seconds are simulation-world cycles / 150,000,000. They are separate from host execution time.
