# Carla experiment measurements

A/A median absolute paired CPU ratio deviation: 1.71%.

| Comparison | Baseline → candidate | Metric | Pairs | Ratio [95% CI] | Flag |
|---|---|---|---:|---|---|
| A/A repeatability | controls-reference-native-empty-1024-continuous → controls-reference-native-repeat-empty-1024-continuous | mean_caller_cpu_ns | 10 | 1.004 [0.987, 1.022] | False |
| Timing instrumentation overhead | controls-reference-native-bare-empty-1024-continuous → controls-reference-native-empty-1024-continuous | mean_caller_cpu_ns | 10 | 2.092 [1.898, 2.279] | True |
| A/A repeatability | controls-reference-native-synthetic-1024-continuous → controls-reference-native-repeat-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 0.998 [0.984, 1.010] | False |
| Timing instrumentation overhead | controls-reference-native-bare-synthetic-1024-continuous → controls-reference-native-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 0.990 [0.972, 1.005] | False |
| A/A repeatability | controls-reference-native-empty-128-continuous → controls-reference-native-repeat-empty-128-continuous | mean_caller_cpu_ns | 10 | 0.944 [0.874, 1.012] | False |
| Timing instrumentation overhead | controls-reference-native-bare-empty-128-continuous → controls-reference-native-empty-128-continuous | mean_caller_cpu_ns | 10 | 2.656 [2.388, 2.991] | True |
| A/A repeatability | controls-reference-native-synthetic-128-continuous → controls-reference-native-repeat-synthetic-128-continuous | mean_caller_cpu_ns | 10 | 1.006 [0.982, 1.034] | False |
| Timing instrumentation overhead | controls-reference-native-bare-synthetic-128-continuous → controls-reference-native-synthetic-128-continuous | mean_caller_cpu_ns | 10 | 1.006 [0.988, 1.021] | False |
| Build-target recipe | abi-reference-native-empty-1024-continuous → abi-packaged-native-empty-1024-continuous | mean_caller_cpu_ns | 10 | 0.966 [0.912, 1.030] | False |
| Build-target recipe | abi-reference-lv2-empty-1024-continuous → abi-packaged-lv2-empty-1024-continuous | mean_caller_cpu_ns | 10 | 1.269 [1.035, 1.656] | True |
| LV2 versus native ABI | abi-reference-native-empty-1024-continuous → abi-reference-lv2-empty-1024-continuous | mean_caller_cpu_ns | 10 | 1.268 [1.235, 1.304] | True |
| Current execution overhead | engine-reference-native-empty-1024-continuous → engine-reference-bridge-empty-1024-continuous | mean_wall_ns | 10 | 16.217 [14.454, 18.211] | True |
| Current inner Carla CPU | engine-reference-native-empty-1024-continuous → engine-reference-bridge-empty-1024-continuous | mean_inner_cpu_ns | 10 | 1.779 [1.502, 2.035] | True |
| Current execution overhead | engine-reference-native-empty-1024-continuous → engine-reference-subprocess-empty-1024-continuous | mean_wall_ns | 10 | 32.913 [28.403, 36.989] | True |
| Current inner Carla CPU | engine-reference-native-empty-1024-continuous → engine-reference-subprocess-empty-1024-continuous | mean_inner_cpu_ns | 10 | 1.855 [1.439, 2.205] | True |
| Current execution overhead | engine-reference-native-empty-1024-continuous → engine-reference-engine-empty-1024-continuous | mean_wall_ns | 10 | 48.157 [43.611, 52.541] | True |
| Current inner Carla CPU | engine-reference-native-empty-1024-continuous → engine-reference-engine-empty-1024-continuous | mean_inner_cpu_ns | 10 | 1.886 [1.638, 2.074] | True |
| Native switch: outer CPU | history-pre-native-empty-1024-continuous → history-post-native-empty-1024-continuous | mean_caller_cpu_ns | 10 | 0.966 [0.771, 1.160] | False |
| Native switch: completion latency | history-pre-native-empty-1024-continuous → history-post-native-empty-1024-continuous | p99_wall_ns | 10 | 0.934 [0.773, 1.119] | False |
| Post-switch versus current | history-post-native-empty-1024-continuous → engine-reference-native-empty-1024-continuous | mean_wall_ns | 10 | 0.932 [0.773, 1.157] | False |
| Native switch: outer CPU | history-pre-bridge-empty-1024-continuous → history-post-bridge-empty-1024-continuous | mean_caller_cpu_ns | 10 | 1.086 [0.963, 1.254] | False |
| Native switch: completion latency | history-pre-bridge-empty-1024-continuous → history-post-bridge-empty-1024-continuous | p99_wall_ns | 10 | 1.289 [0.958, 1.976] | False |
| Post-switch versus current | history-post-bridge-empty-1024-continuous → engine-reference-bridge-empty-1024-continuous | mean_wall_ns | 10 | 0.917 [0.792, 1.022] | False |
| Native switch: outer CPU | history-pre-subprocess-empty-1024-continuous → history-post-subprocess-empty-1024-continuous | mean_caller_cpu_ns | 10 | 2.802 [2.678, 2.968] | True |
| Native switch: completion latency | history-pre-subprocess-empty-1024-continuous → history-post-subprocess-empty-1024-continuous | p99_wall_ns | 10 | 2.362 [2.096, 2.677] | True |
| Post-switch versus current | history-post-subprocess-empty-1024-continuous → engine-reference-subprocess-empty-1024-continuous | mean_wall_ns | 10 | 0.352 [0.338, 0.368] | True |
| Native switch: outer CPU | history-pre-engine-empty-1024-continuous → history-post-engine-empty-1024-continuous | mean_caller_cpu_ns | 10 | 1.005 [0.974, 1.036] | False |
| Native switch: completion latency | history-pre-engine-empty-1024-continuous → history-post-engine-empty-1024-continuous | p99_wall_ns | 10 | 0.981 [0.817, 1.154] | False |
| Post-switch versus current | history-post-engine-empty-1024-continuous → engine-reference-engine-empty-1024-continuous | mean_wall_ns | 10 | 1.076 [0.956, 1.318] | False |
| Build-target recipe | abi-reference-native-gain-1024-continuous → abi-packaged-native-gain-1024-continuous | mean_caller_cpu_ns | 10 | 1.018 [0.914, 1.158] | False |
| Build-target recipe | abi-reference-lv2-gain-1024-continuous → abi-packaged-lv2-gain-1024-continuous | mean_caller_cpu_ns | 10 | 0.966 [0.903, 1.020] | False |
| LV2 versus native ABI | abi-reference-native-gain-1024-continuous → abi-reference-lv2-gain-1024-continuous | mean_caller_cpu_ns | 10 | 0.958 [0.894, 1.013] | False |
| Current execution overhead | engine-reference-native-gain-1024-continuous → engine-reference-bridge-gain-1024-continuous | mean_wall_ns | 10 | 2.196 [2.070, 2.308] | True |
| Current inner Carla CPU | engine-reference-native-gain-1024-continuous → engine-reference-bridge-gain-1024-continuous | mean_inner_cpu_ns | 10 | 1.035 [1.010, 1.061] | False |
| Current execution overhead | engine-reference-native-gain-1024-continuous → engine-reference-subprocess-gain-1024-continuous | mean_wall_ns | 10 | 4.128 [3.602, 5.344] | True |
| Current inner Carla CPU | engine-reference-native-gain-1024-continuous → engine-reference-subprocess-gain-1024-continuous | mean_inner_cpu_ns | 9 | 1.062 [1.035, 1.091] | True |
| Current execution overhead | engine-reference-native-gain-1024-continuous → engine-reference-engine-gain-1024-continuous | mean_wall_ns | 10 | 5.145 [4.605, 6.155] | True |
| Current inner Carla CPU | engine-reference-native-gain-1024-continuous → engine-reference-engine-gain-1024-continuous | mean_inner_cpu_ns | 9 | 1.033 [1.014, 1.050] | False |
| Native switch: outer CPU | history-pre-native-gain-1024-continuous → history-post-native-gain-1024-continuous | mean_caller_cpu_ns | 10 | 1.022 [0.979, 1.070] | False |
| Native switch: completion latency | history-pre-native-gain-1024-continuous → history-post-native-gain-1024-continuous | p99_wall_ns | 10 | 1.300 [0.952, 1.798] | False |
| Post-switch versus current | history-post-native-gain-1024-continuous → engine-reference-native-gain-1024-continuous | mean_wall_ns | 10 | 0.956 [0.915, 1.004] | False |
| Native switch: outer CPU | history-pre-bridge-gain-1024-continuous → history-post-bridge-gain-1024-continuous | mean_caller_cpu_ns | 10 | 0.973 [0.933, 1.012] | False |
| Native switch: completion latency | history-pre-bridge-gain-1024-continuous → history-post-bridge-gain-1024-continuous | p99_wall_ns | 10 | 0.940 [0.800, 1.141] | False |
| Post-switch versus current | history-post-bridge-gain-1024-continuous → engine-reference-bridge-gain-1024-continuous | mean_wall_ns | 10 | 0.971 [0.947, 0.998] | False |
| Native switch: outer CPU | history-pre-subprocess-gain-1024-continuous → history-post-subprocess-gain-1024-continuous | mean_caller_cpu_ns | 10 | 2.108 [1.802, 2.354] | True |
| Native switch: completion latency | history-pre-subprocess-gain-1024-continuous → history-post-subprocess-gain-1024-continuous | p99_wall_ns | 10 | 1.962 [1.340, 2.608] | True |
| Post-switch versus current | history-post-subprocess-gain-1024-continuous → engine-reference-subprocess-gain-1024-continuous | mean_wall_ns | 10 | 0.490 [0.402, 0.677] | True |
| Native switch: outer CPU | history-pre-engine-gain-1024-continuous → history-post-engine-gain-1024-continuous | mean_caller_cpu_ns | 10 | 0.974 [0.933, 1.005] | False |
| Native switch: completion latency | history-pre-engine-gain-1024-continuous → history-post-engine-gain-1024-continuous | p99_wall_ns | 10 | 0.893 [0.728, 1.062] | False |
| Post-switch versus current | history-post-engine-gain-1024-continuous → engine-reference-engine-gain-1024-continuous | mean_wall_ns | 10 | 1.152 [1.011, 1.386] | False |
| Build-target recipe | abi-reference-native-synthetic-1024-continuous → abi-packaged-native-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 1.091 [1.018, 1.204] | False |
| Build-target recipe | abi-reference-lv2-synthetic-1024-continuous → abi-packaged-lv2-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 1.169 [1.011, 1.377] | False |
| LV2 versus native ABI | abi-reference-native-synthetic-1024-continuous → abi-reference-lv2-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 0.982 [0.930, 1.013] | False |
| Current execution overhead | engine-reference-native-synthetic-1024-continuous → engine-reference-bridge-synthetic-1024-continuous | mean_wall_ns | 10 | 1.025 [1.004, 1.045] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-1024-continuous → engine-reference-bridge-synthetic-1024-continuous | mean_inner_cpu_ns | 10 | 0.992 [0.973, 1.010] | False |
| Current execution overhead | engine-reference-native-synthetic-1024-continuous → engine-reference-subprocess-synthetic-1024-continuous | mean_wall_ns | 10 | 1.095 [1.067, 1.124] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-1024-continuous → engine-reference-subprocess-synthetic-1024-continuous | mean_inner_cpu_ns | 10 | 1.013 [0.994, 1.033] | False |
| Current execution overhead | engine-reference-native-synthetic-1024-continuous → engine-reference-engine-synthetic-1024-continuous | mean_wall_ns | 10 | 1.067 [1.049, 1.084] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-1024-continuous → engine-reference-engine-synthetic-1024-continuous | mean_inner_cpu_ns | 10 | 0.986 [0.969, 1.001] | False |
| Native switch: outer CPU | history-pre-native-synthetic-1024-continuous → history-post-native-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 1.002 [0.979, 1.026] | False |
| Native switch: completion latency | history-pre-native-synthetic-1024-continuous → history-post-native-synthetic-1024-continuous | p99_wall_ns | 10 | 0.972 [0.783, 1.227] | False |
| Post-switch versus current | history-post-native-synthetic-1024-continuous → engine-reference-native-synthetic-1024-continuous | mean_wall_ns | 10 | 0.999 [0.975, 1.026] | False |
| Native switch: outer CPU | history-pre-bridge-synthetic-1024-continuous → history-post-bridge-synthetic-1024-continuous | mean_caller_cpu_ns | 9 | 1.009 [0.983, 1.039] | False |
| Native switch: completion latency | history-pre-bridge-synthetic-1024-continuous → history-post-bridge-synthetic-1024-continuous | p99_wall_ns | 10 | 1.153 [0.879, 1.533] | False |
| Post-switch versus current | history-post-bridge-synthetic-1024-continuous → engine-reference-bridge-synthetic-1024-continuous | mean_wall_ns | 10 | 0.968 [0.947, 0.989] | False |
| Native switch: outer CPU | history-pre-subprocess-synthetic-1024-continuous → history-post-subprocess-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 1.098 [1.051, 1.140] | True |
| Native switch: completion latency | history-pre-subprocess-synthetic-1024-continuous → history-post-subprocess-synthetic-1024-continuous | p99_wall_ns | 10 | 1.029 [0.818, 1.254] | False |
| Post-switch versus current | history-post-subprocess-synthetic-1024-continuous → engine-reference-subprocess-synthetic-1024-continuous | mean_wall_ns | 10 | 0.894 [0.856, 0.948] | False |
| Native switch: outer CPU | history-pre-engine-synthetic-1024-continuous → history-post-engine-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 0.988 [0.936, 1.048] | False |
| Native switch: completion latency | history-pre-engine-synthetic-1024-continuous → history-post-engine-synthetic-1024-continuous | p99_wall_ns | 10 | 0.900 [0.643, 1.296] | False |
| Post-switch versus current | history-post-engine-synthetic-1024-continuous → engine-reference-engine-synthetic-1024-continuous | mean_wall_ns | 10 | 0.962 [0.912, 1.000] | False |
| Build-target recipe | abi-reference-native-empty-256-continuous → abi-packaged-native-empty-256-continuous | mean_caller_cpu_ns | 10 | 0.965 [0.889, 1.040] | False |
| Build-target recipe | abi-reference-lv2-empty-256-continuous → abi-packaged-lv2-empty-256-continuous | mean_caller_cpu_ns | 10 | 0.937 [0.750, 1.132] | False |
| LV2 versus native ABI | abi-reference-native-empty-256-continuous → abi-reference-lv2-empty-256-continuous | mean_caller_cpu_ns | 10 | 1.722 [1.297, 2.563] | True |
| Current execution overhead | engine-reference-native-empty-256-continuous → engine-reference-bridge-empty-256-continuous | mean_wall_ns | 10 | 19.058 [17.063, 21.441] | True |
| Current inner Carla CPU | engine-reference-native-empty-256-continuous → engine-reference-bridge-empty-256-continuous | mean_inner_cpu_ns | 10 | 2.071 [1.831, 2.266] | True |
| Current execution overhead | engine-reference-native-empty-256-continuous → engine-reference-subprocess-empty-256-continuous | mean_wall_ns | 10 | 38.930 [33.773, 48.120] | True |
| Current inner Carla CPU | engine-reference-native-empty-256-continuous → engine-reference-subprocess-empty-256-continuous | mean_inner_cpu_ns | 10 | 2.238 [1.904, 2.616] | True |
| Current execution overhead | engine-reference-native-empty-256-continuous → engine-reference-engine-empty-256-continuous | mean_wall_ns | 10 | 27.571 [26.204, 28.917] | True |
| Current inner Carla CPU | engine-reference-native-empty-256-continuous → engine-reference-engine-empty-256-continuous | mean_inner_cpu_ns | 10 | 1.967 [1.779, 2.130] | True |
| Native switch: outer CPU | history-pre-native-empty-256-continuous → history-post-native-empty-256-continuous | mean_caller_cpu_ns | 10 | 0.815 [0.705, 0.963] | False |
| Native switch: completion latency | history-pre-native-empty-256-continuous → history-post-native-empty-256-continuous | p99_wall_ns | 10 | 0.907 [0.755, 1.122] | False |
| Post-switch versus current | history-post-native-empty-256-continuous → engine-reference-native-empty-256-continuous | mean_wall_ns | 10 | 0.926 [0.841, 1.013] | False |
| Native switch: outer CPU | history-pre-bridge-empty-256-continuous → history-post-bridge-empty-256-continuous | mean_caller_cpu_ns | 10 | 0.991 [0.940, 1.068] | False |
| Native switch: completion latency | history-pre-bridge-empty-256-continuous → history-post-bridge-empty-256-continuous | p99_wall_ns | 10 | 0.956 [0.878, 1.053] | False |
| Post-switch versus current | history-post-bridge-empty-256-continuous → engine-reference-bridge-empty-256-continuous | mean_wall_ns | 10 | 1.045 [0.908, 1.212] | False |
| Native switch: outer CPU | history-pre-subprocess-empty-256-continuous → history-post-subprocess-empty-256-continuous | mean_caller_cpu_ns | 8 | 2.686 [2.446, 2.992] | True |
| Native switch: completion latency | history-pre-subprocess-empty-256-continuous → history-post-subprocess-empty-256-continuous | p99_wall_ns | 9 | 2.094 [1.661, 2.627] | True |
| Post-switch versus current | history-post-subprocess-empty-256-continuous → engine-reference-subprocess-empty-256-continuous | mean_wall_ns | 9 | 0.398 [0.316, 0.540] | True |
| Native switch: outer CPU | history-pre-engine-empty-256-continuous → history-post-engine-empty-256-continuous | mean_caller_cpu_ns | 10 | 1.053 [0.892, 1.349] | False |
| Native switch: completion latency | history-pre-engine-empty-256-continuous → history-post-engine-empty-256-continuous | p99_wall_ns | 10 | 1.079 [0.644, 2.251] | False |
| Post-switch versus current | history-post-engine-empty-256-continuous → engine-reference-engine-empty-256-continuous | mean_wall_ns | 10 | 0.920 [0.742, 1.053] | False |
| Build-target recipe | abi-reference-native-gain-256-continuous → abi-packaged-native-gain-256-continuous | mean_caller_cpu_ns | 10 | 0.959 [0.825, 1.119] | False |
| Build-target recipe | abi-reference-lv2-gain-256-continuous → abi-packaged-lv2-gain-256-continuous | mean_caller_cpu_ns | 10 | 0.977 [0.874, 1.121] | False |
| LV2 versus native ABI | abi-reference-native-gain-256-continuous → abi-reference-lv2-gain-256-continuous | mean_caller_cpu_ns | 10 | 1.040 [0.962, 1.126] | False |
| Current execution overhead | engine-reference-native-gain-256-continuous → engine-reference-bridge-gain-256-continuous | mean_wall_ns | 10 | 2.799 [2.687, 2.906] | True |
| Current inner Carla CPU | engine-reference-native-gain-256-continuous → engine-reference-bridge-gain-256-continuous | mean_inner_cpu_ns | 10 | 0.989 [0.884, 1.063] | False |
| Current execution overhead | engine-reference-native-gain-256-continuous → engine-reference-subprocess-gain-256-continuous | mean_wall_ns | 10 | 4.793 [4.350, 5.424] | True |
| Current inner Carla CPU | engine-reference-native-gain-256-continuous → engine-reference-subprocess-gain-256-continuous | mean_inner_cpu_ns | 9 | 1.047 [1.033, 1.058] | False |
| Current execution overhead | engine-reference-native-gain-256-continuous → engine-reference-engine-gain-256-continuous | mean_wall_ns | 10 | 3.988 [3.838, 4.126] | True |
| Current inner Carla CPU | engine-reference-native-gain-256-continuous → engine-reference-engine-gain-256-continuous | mean_inner_cpu_ns | 10 | 0.986 [0.909, 1.036] | False |
| Native switch: outer CPU | history-pre-native-gain-256-continuous → history-post-native-gain-256-continuous | mean_caller_cpu_ns | 10 | 0.956 [0.905, 1.029] | False |
| Native switch: completion latency | history-pre-native-gain-256-continuous → history-post-native-gain-256-continuous | p99_wall_ns | 10 | 0.779 [0.611, 1.002] | False |
| Post-switch versus current | history-post-native-gain-256-continuous → engine-reference-native-gain-256-continuous | mean_wall_ns | 10 | 1.022 [0.906, 1.204] | False |
| Native switch: outer CPU | history-pre-bridge-gain-256-continuous → history-post-bridge-gain-256-continuous | mean_caller_cpu_ns | 10 | 1.146 [1.007, 1.410] | False |
| Native switch: completion latency | history-pre-bridge-gain-256-continuous → history-post-bridge-gain-256-continuous | p99_wall_ns | 10 | 1.506 [1.027, 2.646] | False |
| Post-switch versus current | history-post-bridge-gain-256-continuous → engine-reference-bridge-gain-256-continuous | mean_wall_ns | 10 | 0.891 [0.694, 1.057] | False |
| Native switch: outer CPU | history-pre-subprocess-gain-256-continuous → history-post-subprocess-gain-256-continuous | mean_caller_cpu_ns | 5 | 2.400 [2.291, 2.490] | False |
| Native switch: completion latency | history-pre-subprocess-gain-256-continuous → history-post-subprocess-gain-256-continuous | p99_wall_ns | 5 | 2.276 [1.820, 2.739] | False |
| Post-switch versus current | history-post-subprocess-gain-256-continuous → engine-reference-subprocess-gain-256-continuous | mean_wall_ns | 6 | 0.581 [0.361, 1.116] | False |
| Native switch: outer CPU | history-pre-engine-gain-256-continuous → history-post-engine-gain-256-continuous | mean_caller_cpu_ns | 9 | 1.003 [0.970, 1.037] | False |
| Native switch: completion latency | history-pre-engine-gain-256-continuous → history-post-engine-gain-256-continuous | p99_wall_ns | 10 | 1.148 [0.969, 1.314] | False |
| Post-switch versus current | history-post-engine-gain-256-continuous → engine-reference-engine-gain-256-continuous | mean_wall_ns | 10 | 1.025 [0.933, 1.164] | False |
| Build-target recipe | abi-reference-native-synthetic-256-continuous → abi-packaged-native-synthetic-256-continuous | mean_caller_cpu_ns | 10 | 0.995 [0.946, 1.043] | False |
| Build-target recipe | abi-reference-lv2-synthetic-256-continuous → abi-packaged-lv2-synthetic-256-continuous | mean_caller_cpu_ns | 10 | 1.050 [0.996, 1.136] | False |
| LV2 versus native ABI | abi-reference-native-synthetic-256-continuous → abi-reference-lv2-synthetic-256-continuous | mean_caller_cpu_ns | 10 | 0.964 [0.927, 0.996] | False |
| Current execution overhead | engine-reference-native-synthetic-256-continuous → engine-reference-bridge-synthetic-256-continuous | mean_wall_ns | 10 | 1.086 [1.016, 1.132] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-256-continuous → engine-reference-bridge-synthetic-256-continuous | mean_inner_cpu_ns | 10 | 0.972 [0.910, 1.012] | False |
| Current execution overhead | engine-reference-native-synthetic-256-continuous → engine-reference-subprocess-synthetic-256-continuous | mean_wall_ns | 10 | 1.199 [1.148, 1.236] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-256-continuous → engine-reference-subprocess-synthetic-256-continuous | mean_inner_cpu_ns | 9 | 1.010 [0.997, 1.022] | False |
| Current execution overhead | engine-reference-native-synthetic-256-continuous → engine-reference-engine-synthetic-256-continuous | mean_wall_ns | 10 | 1.201 [1.134, 1.296] | True |
| Current inner Carla CPU | engine-reference-native-synthetic-256-continuous → engine-reference-engine-synthetic-256-continuous | mean_inner_cpu_ns | 10 | 0.972 [0.929, 1.003] | False |
| Native switch: outer CPU | history-pre-native-synthetic-256-continuous → history-post-native-synthetic-256-continuous | mean_caller_cpu_ns | 10 | 0.967 [0.924, 1.000] | False |
| Native switch: completion latency | history-pre-native-synthetic-256-continuous → history-post-native-synthetic-256-continuous | p99_wall_ns | 10 | 0.887 [0.655, 1.135] | False |
| Post-switch versus current | history-post-native-synthetic-256-continuous → engine-reference-native-synthetic-256-continuous | mean_wall_ns | 10 | 1.024 [0.978, 1.093] | False |
| Native switch: outer CPU | history-pre-bridge-synthetic-256-continuous → history-post-bridge-synthetic-256-continuous | mean_caller_cpu_ns | 8 | 0.987 [0.973, 1.001] | False |
| Native switch: completion latency | history-pre-bridge-synthetic-256-continuous → history-post-bridge-synthetic-256-continuous | p99_wall_ns | 10 | 0.958 [0.562, 1.662] | False |
| Post-switch versus current | history-post-bridge-synthetic-256-continuous → engine-reference-bridge-synthetic-256-continuous | mean_wall_ns | 10 | 0.990 [0.950, 1.020] | False |
| Native switch: outer CPU | history-pre-subprocess-synthetic-256-continuous → history-post-subprocess-synthetic-256-continuous | mean_caller_cpu_ns | 6 | 1.331 [1.266, 1.413] | False |
| Native switch: completion latency | history-pre-subprocess-synthetic-256-continuous → history-post-subprocess-synthetic-256-continuous | p99_wall_ns | 8 | 1.130 [0.936, 1.411] | False |
| Post-switch versus current | history-post-subprocess-synthetic-256-continuous → engine-reference-subprocess-synthetic-256-continuous | mean_wall_ns | 10 | 0.723 [0.694, 0.750] | True |
| Native switch: outer CPU | history-pre-engine-synthetic-256-continuous → history-post-engine-synthetic-256-continuous | mean_caller_cpu_ns | 10 | 0.972 [0.931, 1.006] | False |
| Native switch: completion latency | history-pre-engine-synthetic-256-continuous → history-post-engine-synthetic-256-continuous | p99_wall_ns | 10 | 0.925 [0.735, 1.139] | False |
| Post-switch versus current | history-post-engine-synthetic-256-continuous → engine-reference-engine-synthetic-256-continuous | mean_wall_ns | 10 | 1.048 [0.967, 1.150] | False |
| Build-target recipe | abi-reference-native-empty-128-continuous → abi-packaged-native-empty-128-continuous | mean_caller_cpu_ns | 10 | 1.013 [0.952, 1.079] | False |
| Build-target recipe | abi-reference-lv2-empty-128-continuous → abi-packaged-lv2-empty-128-continuous | mean_caller_cpu_ns | 10 | 0.955 [0.881, 1.044] | False |
| LV2 versus native ABI | abi-reference-native-empty-128-continuous → abi-reference-lv2-empty-128-continuous | mean_caller_cpu_ns | 10 | 1.387 [1.312, 1.459] | True |
| Current execution overhead | engine-reference-native-empty-128-continuous → engine-reference-bridge-empty-128-continuous | mean_wall_ns | 10 | 17.141 [15.626, 18.416] | True |
| Current inner Carla CPU | engine-reference-native-empty-128-continuous → engine-reference-bridge-empty-128-continuous | mean_inner_cpu_ns | 10 | 2.042 [1.931, 2.157] | True |
| Current execution overhead | engine-reference-native-empty-128-continuous → engine-reference-subprocess-empty-128-continuous | mean_wall_ns | 10 | 27.525 [22.886, 32.366] | True |
| Current inner Carla CPU | engine-reference-native-empty-128-continuous → engine-reference-subprocess-empty-128-continuous | mean_inner_cpu_ns | 9 | 2.310 [2.106, 2.583] | True |
| Current execution overhead | engine-reference-native-empty-128-continuous → engine-reference-engine-empty-128-continuous | mean_wall_ns | 10 | 23.976 [20.771, 27.888] | True |
| Current inner Carla CPU | engine-reference-native-empty-128-continuous → engine-reference-engine-empty-128-continuous | mean_inner_cpu_ns | 9 | 2.087 [1.900, 2.274] | True |
| Native switch: outer CPU | history-pre-native-empty-128-continuous → history-post-native-empty-128-continuous | mean_caller_cpu_ns | 10 | 0.968 [0.847, 1.135] | False |
| Native switch: completion latency | history-pre-native-empty-128-continuous → history-post-native-empty-128-continuous | p99_wall_ns | 10 | 1.029 [0.876, 1.217] | False |
| Post-switch versus current | history-post-native-empty-128-continuous → engine-reference-native-empty-128-continuous | mean_wall_ns | 10 | 0.872 [0.750, 1.018] | False |
| Native switch: outer CPU | history-pre-bridge-empty-128-continuous → history-post-bridge-empty-128-continuous | mean_caller_cpu_ns | 8 | 0.967 [0.916, 1.011] | False |
| Native switch: completion latency | history-pre-bridge-empty-128-continuous → history-post-bridge-empty-128-continuous | p99_wall_ns | 10 | 0.886 [0.705, 1.085] | False |
| Post-switch versus current | history-post-bridge-empty-128-continuous → engine-reference-bridge-empty-128-continuous | mean_wall_ns | 10 | 0.953 [0.893, 1.014] | False |
| Native switch: outer CPU | history-pre-subprocess-empty-128-continuous → history-post-subprocess-empty-128-continuous | mean_caller_cpu_ns | 1 | insufficient valid pairs | — |
| Native switch: completion latency | history-pre-subprocess-empty-128-continuous → history-post-subprocess-empty-128-continuous | p99_wall_ns | 1 | insufficient valid pairs | — |
| Post-switch versus current | history-post-subprocess-empty-128-continuous → engine-reference-subprocess-empty-128-continuous | mean_wall_ns | 6 | 0.469 [0.229, 1.311] | False |
| Native switch: outer CPU | history-pre-engine-empty-128-continuous → history-post-engine-empty-128-continuous | mean_caller_cpu_ns | 9 | 0.912 [0.829, 0.992] | False |
| Native switch: completion latency | history-pre-engine-empty-128-continuous → history-post-engine-empty-128-continuous | p99_wall_ns | 10 | 1.003 [0.649, 1.647] | False |
| Post-switch versus current | history-post-engine-empty-128-continuous → engine-reference-engine-empty-128-continuous | mean_wall_ns | 10 | 1.028 [0.879, 1.176] | False |
| 0.2.0 versus pre-switch inner CPU (both paced) | legacy-020-lv2-empty-128-continuous → history-pre-native-empty-128-paced | comparable_inner_cpu_ns | 10 | 0.582 [0.516, 0.657] | True |
| Build-target recipe | abi-reference-native-gain-128-continuous → abi-packaged-native-gain-128-continuous | mean_caller_cpu_ns | 10 | 1.008 [0.963, 1.048] | False |
| Build-target recipe | abi-reference-lv2-gain-128-continuous → abi-packaged-lv2-gain-128-continuous | mean_caller_cpu_ns | 10 | 1.011 [0.960, 1.052] | False |
| LV2 versus native ABI | abi-reference-native-gain-128-continuous → abi-reference-lv2-gain-128-continuous | mean_caller_cpu_ns | 10 | 1.017 [0.949, 1.088] | False |
| Current execution overhead | engine-reference-native-gain-128-continuous → engine-reference-bridge-gain-128-continuous | mean_wall_ns | 10 | 3.333 [3.150, 3.520] | True |
| Current inner Carla CPU | engine-reference-native-gain-128-continuous → engine-reference-bridge-gain-128-continuous | mean_inner_cpu_ns | 10 | 1.093 [1.055, 1.135] | True |
| Current execution overhead | engine-reference-native-gain-128-continuous → engine-reference-subprocess-gain-128-continuous | mean_wall_ns | 10 | 5.172 [4.723, 5.578] | True |
| Current inner Carla CPU | engine-reference-native-gain-128-continuous → engine-reference-subprocess-gain-128-continuous | mean_inner_cpu_ns | 9 | 1.071 [1.049, 1.093] | True |
| Current execution overhead | engine-reference-native-gain-128-continuous → engine-reference-engine-gain-128-continuous | mean_wall_ns | 10 | 4.083 [3.796, 4.558] | True |
| Current inner Carla CPU | engine-reference-native-gain-128-continuous → engine-reference-engine-gain-128-continuous | mean_inner_cpu_ns | 10 | 1.062 [1.022, 1.106] | True |
| Native switch: outer CPU | history-pre-native-gain-128-continuous → history-post-native-gain-128-continuous | mean_caller_cpu_ns | 10 | 0.997 [0.949, 1.050] | False |
| Native switch: completion latency | history-pre-native-gain-128-continuous → history-post-native-gain-128-continuous | p99_wall_ns | 10 | 1.102 [0.926, 1.323] | False |
| Post-switch versus current | history-post-native-gain-128-continuous → engine-reference-native-gain-128-continuous | mean_wall_ns | 10 | 0.928 [0.890, 0.964] | False |
| Native switch: outer CPU | history-pre-bridge-gain-128-continuous → history-post-bridge-gain-128-continuous | mean_caller_cpu_ns | 10 | 0.937 [0.827, 1.037] | False |
| Native switch: completion latency | history-pre-bridge-gain-128-continuous → history-post-bridge-gain-128-continuous | p99_wall_ns | 10 | 0.750 [0.534, 0.964] | True |
| Post-switch versus current | history-post-bridge-gain-128-continuous → engine-reference-bridge-gain-128-continuous | mean_wall_ns | 10 | 1.019 [0.935, 1.088] | False |
| Native switch: outer CPU | history-pre-subprocess-gain-128-continuous → history-post-subprocess-gain-128-continuous | mean_caller_cpu_ns | 0 | insufficient valid pairs | — |
| Native switch: completion latency | history-pre-subprocess-gain-128-continuous → history-post-subprocess-gain-128-continuous | p99_wall_ns | 0 | insufficient valid pairs | — |
| Post-switch versus current | history-post-subprocess-gain-128-continuous → engine-reference-subprocess-gain-128-continuous | mean_wall_ns | 2 | 0.301 [0.293, 0.308] | False |
| Native switch: outer CPU | history-pre-engine-gain-128-continuous → history-post-engine-gain-128-continuous | mean_caller_cpu_ns | 10 | 0.973 [0.907, 1.034] | False |
| Native switch: completion latency | history-pre-engine-gain-128-continuous → history-post-engine-gain-128-continuous | p99_wall_ns | 10 | 1.004 [0.814, 1.210] | False |
| Post-switch versus current | history-post-engine-gain-128-continuous → engine-reference-engine-gain-128-continuous | mean_wall_ns | 10 | 1.050 [0.974, 1.171] | False |
| 0.2.0 versus pre-switch inner CPU (both paced) | legacy-020-lv2-gain-128-continuous → history-pre-native-gain-128-paced | comparable_inner_cpu_ns | 10 | 0.741 [0.654, 0.843] | True |
| Build-target recipe | abi-reference-native-synthetic-128-continuous → abi-packaged-native-synthetic-128-continuous | mean_caller_cpu_ns | 10 | 0.876 [0.617, 1.077] | False |
| Build-target recipe | abi-reference-lv2-synthetic-128-continuous → abi-packaged-lv2-synthetic-128-continuous | mean_caller_cpu_ns | 10 | 0.952 [0.903, 0.996] | False |
| LV2 versus native ABI | abi-reference-native-synthetic-128-continuous → abi-reference-lv2-synthetic-128-continuous | mean_caller_cpu_ns | 10 | 0.911 [0.646, 1.139] | False |
| Current execution overhead | engine-reference-native-synthetic-128-continuous → engine-reference-bridge-synthetic-128-continuous | mean_wall_ns | 10 | 1.173 [1.149, 1.197] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-128-continuous → engine-reference-bridge-synthetic-128-continuous | mean_inner_cpu_ns | 8 | 0.985 [0.962, 1.007] | False |
| Current execution overhead | engine-reference-native-synthetic-128-continuous → engine-reference-subprocess-synthetic-128-continuous | mean_wall_ns | 10 | 1.353 [1.227, 1.471] | True |
| Current inner Carla CPU | engine-reference-native-synthetic-128-continuous → engine-reference-subprocess-synthetic-128-continuous | mean_inner_cpu_ns | 8 | 0.945 [0.856, 1.005] | False |
| Current execution overhead | engine-reference-native-synthetic-128-continuous → engine-reference-engine-synthetic-128-continuous | mean_wall_ns | 10 | 1.191 [1.105, 1.250] | False |
| Current inner Carla CPU | engine-reference-native-synthetic-128-continuous → engine-reference-engine-synthetic-128-continuous | mean_inner_cpu_ns | 10 | 0.948 [0.882, 0.992] | False |
| Native switch: outer CPU | history-pre-native-synthetic-128-continuous → history-post-native-synthetic-128-continuous | mean_caller_cpu_ns | 10 | 1.000 [0.975, 1.024] | False |
| Native switch: completion latency | history-pre-native-synthetic-128-continuous → history-post-native-synthetic-128-continuous | p99_wall_ns | 10 | 0.995 [0.851, 1.161] | False |
| Post-switch versus current | history-post-native-synthetic-128-continuous → engine-reference-native-synthetic-128-continuous | mean_wall_ns | 10 | 1.035 [0.988, 1.114] | False |
| Native switch: outer CPU | history-pre-bridge-synthetic-128-continuous → history-post-bridge-synthetic-128-continuous | mean_caller_cpu_ns | 9 | 1.003 [0.981, 1.028] | False |
| Native switch: completion latency | history-pre-bridge-synthetic-128-continuous → history-post-bridge-synthetic-128-continuous | p99_wall_ns | 10 | 1.282 [0.967, 1.882] | False |
| Post-switch versus current | history-post-bridge-synthetic-128-continuous → engine-reference-bridge-synthetic-128-continuous | mean_wall_ns | 10 | 1.067 [0.983, 1.190] | False |
| Native switch: outer CPU | history-pre-subprocess-synthetic-128-continuous → history-post-subprocess-synthetic-128-continuous | mean_caller_cpu_ns | 2 | 1.464 [1.414, 1.517] | False |
| Native switch: completion latency | history-pre-subprocess-synthetic-128-continuous → history-post-subprocess-synthetic-128-continuous | p99_wall_ns | 2 | 1.091 [1.039, 1.145] | False |
| Post-switch versus current | history-post-subprocess-synthetic-128-continuous → engine-reference-subprocess-synthetic-128-continuous | mean_wall_ns | 6 | 0.658 [0.634, 0.688] | False |
| Native switch: outer CPU | history-pre-engine-synthetic-128-continuous → history-post-engine-synthetic-128-continuous | mean_caller_cpu_ns | 7 | 1.008 [0.961, 1.062] | False |
| Native switch: completion latency | history-pre-engine-synthetic-128-continuous → history-post-engine-synthetic-128-continuous | p99_wall_ns | 10 | 0.936 [0.797, 1.129] | False |
| Post-switch versus current | history-post-engine-synthetic-128-continuous → engine-reference-engine-synthetic-128-continuous | mean_wall_ns | 10 | 1.398 [0.996, 2.067] | False |
| 0.2.0 versus pre-switch inner CPU (both paced) | legacy-020-lv2-synthetic-128-continuous → history-pre-native-synthetic-128-paced | comparable_inner_cpu_ns | 10 | 1.035 [0.992, 1.093] | False |
| Controlled CPU contention | engine-reference-native-synthetic-1024-paced → engine-reference-native-synthetic-1024-paced-loaded | p99_wall_ns | 10 | 1.800 [1.078, 2.998] | False |
| CPU cost under contention | engine-reference-native-synthetic-1024-paced → engine-reference-native-synthetic-1024-paced-loaded | mean_inner_cpu_ns | 10 | 0.963 [0.942, 0.985] | False |
| Inner wall time under contention | engine-reference-native-synthetic-1024-paced → engine-reference-native-synthetic-1024-paced-loaded | p99_inner_wall_ns | 10 | 1.844 [1.110, 3.027] | False |
| Controlled CPU contention | engine-reference-bridge-synthetic-1024-paced → engine-reference-bridge-synthetic-1024-paced-loaded | p99_wall_ns | 10 | 1.928 [1.804, 2.051] | True |
| CPU cost under contention | engine-reference-bridge-synthetic-1024-paced → engine-reference-bridge-synthetic-1024-paced-loaded | mean_inner_cpu_ns | 10 | 0.962 [0.933, 0.987] | False |
| Inner wall time under contention | engine-reference-bridge-synthetic-1024-paced → engine-reference-bridge-synthetic-1024-paced-loaded | p99_inner_wall_ns | 10 | 0.830 [0.639, 1.031] | False |
| Controlled CPU contention | engine-reference-subprocess-synthetic-1024-paced → engine-reference-subprocess-synthetic-1024-paced-loaded | p99_wall_ns | 10 | 1.609 [1.530, 1.699] | True |
| CPU cost under contention | engine-reference-subprocess-synthetic-1024-paced → engine-reference-subprocess-synthetic-1024-paced-loaded | mean_inner_cpu_ns | 8 | 0.947 [0.916, 0.969] | True |
| Inner wall time under contention | engine-reference-subprocess-synthetic-1024-paced → engine-reference-subprocess-synthetic-1024-paced-loaded | p99_inner_wall_ns | 10 | 0.632 [0.406, 0.859] | True |
| Controlled CPU contention | engine-reference-engine-synthetic-1024-paced → engine-reference-engine-synthetic-1024-paced-loaded | p99_wall_ns | 10 | 2.122 [1.939, 2.400] | True |
| CPU cost under contention | engine-reference-engine-synthetic-1024-paced → engine-reference-engine-synthetic-1024-paced-loaded | mean_inner_cpu_ns | 9 | 0.972 [0.956, 0.995] | False |
| Inner wall time under contention | engine-reference-engine-synthetic-1024-paced → engine-reference-engine-synthetic-1024-paced-loaded | p99_inner_wall_ns | 9 | 1.062 [0.778, 1.708] | False |
| Adjacent native-switch confirmation | history-pre-subprocess-synthetic-1024-continuous → history-post-subprocess-synthetic-1024-continuous | mean_caller_cpu_ns | 10 | 1.124 [1.089, 1.160] | True |
| Adjacent native-switch confirmation | history-pre-subprocess-synthetic-1024-continuous → history-post-subprocess-synthetic-1024-continuous | mean_inner_cpu_ns | 10 | 1.019 [0.999, 1.042] | False |
| Adjacent native-switch confirmation | history-pre-subprocess-synthetic-1024-continuous → history-post-subprocess-synthetic-1024-continuous | p99_wall_ns | 10 | 1.073 [0.893, 1.267] | False |
| Voyage historical versus current | history-pre-native-voyage-1024-continuous → engine-reference-native-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.014 [0.995, 1.033] | False |
| Voyage historical versus current | history-post-native-voyage-1024-continuous → engine-reference-native-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.974 [0.952, 0.997] | False |
| Voyage native switch | history-pre-native-voyage-1024-continuous → history-post-native-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.041 [1.016, 1.066] | False |
| Voyage execution paths | engine-reference-native-voyage-1024-continuous → engine-reference-native-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.000 [1.000, 1.000] | False |
| Voyage historical versus current | history-pre-bridge-voyage-1024-continuous → engine-reference-bridge-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.912 [0.791, 1.039] | False |
| Voyage historical versus current | history-post-bridge-voyage-1024-continuous → engine-reference-bridge-voyage-1024-continuous | mean_inner_cpu_ns | 9 | 0.884 [0.775, 1.001] | False |
| Voyage native switch | history-pre-bridge-voyage-1024-continuous → history-post-bridge-voyage-1024-continuous | mean_inner_cpu_ns | 9 | 1.079 [0.921, 1.288] | False |
| Voyage execution paths | engine-reference-native-voyage-1024-continuous → engine-reference-bridge-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.998 [0.959, 1.045] | False |
| Voyage historical versus current | history-pre-subprocess-voyage-1024-continuous → engine-reference-subprocess-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.067 [0.914, 1.245] | False |
| Voyage historical versus current | history-post-subprocess-voyage-1024-continuous → engine-reference-subprocess-voyage-1024-continuous | mean_inner_cpu_ns | 9 | 1.017 [0.883, 1.140] | False |
| Voyage native switch | history-pre-subprocess-voyage-1024-continuous → history-post-subprocess-voyage-1024-continuous | mean_inner_cpu_ns | 9 | 1.088 [0.905, 1.332] | False |
| Voyage execution paths | engine-reference-native-voyage-1024-continuous → engine-reference-subprocess-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.277 [1.113, 1.456] | False |
| Voyage historical versus current | history-pre-engine-voyage-1024-continuous → engine-reference-engine-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.969 [0.855, 1.081] | False |
| Voyage historical versus current | history-post-engine-voyage-1024-continuous → engine-reference-engine-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.979 [0.881, 1.077] | False |
| Voyage native switch | history-pre-engine-voyage-1024-continuous → history-post-engine-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.991 [0.907, 1.070] | False |
| Voyage execution paths | engine-reference-native-voyage-1024-continuous → engine-reference-engine-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.040 [0.974, 1.119] | False |
| Voyage 0.2.0 versus current (cadence differs) | legacy-020-lv2-voyage-1024-paced → engine-reference-native-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.819 [0.680, 0.933] | True |
| Voyage 0.2.0 versus current (cadence differs) | legacy-020-lv2-voyage-1024-paced → engine-reference-bridge-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.817 [0.678, 0.925] | True |
| Voyage 0.2.0 versus current (cadence differs) | legacy-020-lv2-voyage-1024-paced → engine-reference-subprocess-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 1.046 [0.801, 1.278] | False |
| Voyage 0.2.0 versus current (cadence differs) | legacy-020-lv2-voyage-1024-paced → engine-reference-engine-voyage-1024-continuous | mean_inner_cpu_ns | 10 | 0.852 [0.728, 0.971] | False |
| Voyage matched cadence versus 0.2.0 | legacy-020-lv2-voyage-1024-paced → history-pre-native-voyage-1024-paced | mean_inner_cpu_ns | 10 | 0.909 [0.730, 1.061] | False |
| Voyage matched cadence versus 0.2.0 | legacy-020-lv2-voyage-1024-paced → engine-reference-native-voyage-1024-paced | mean_inner_cpu_ns | 10 | 0.895 [0.728, 1.044] | False |
| Voyage matched cadence versus 0.2.0 | legacy-020-lv2-voyage-1024-paced → engine-reference-subprocess-voyage-1024-paced | mean_inner_cpu_ns | 9 | 1.062 [0.981, 1.167] | False |

Flags are screening results under the predeclared thresholds, not causal diagnoses.
CPU comparisons exclude runs with invalid output, deadline misses, or incomplete inner-call matching; latency summaries retain deadline/fallback effects.
Old 0.2.0 samples are inner LV2 call timings under its internally paced dummy driver, not full callback timings.
