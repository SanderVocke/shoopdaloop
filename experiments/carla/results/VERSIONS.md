# Previous stable Carla version comparison

Ratio is 2.5.10 / 2.5.9; same host/plugin/input/FP policy. Linux source-build sensitivity, not a historical Mac binary reproduction.

| Interface | FTZ/DAZ | Metric | Pairs | Ratio [95% CI] | Screen |
|---|---:|---|---:|---|---|
| native | 0 | mean_caller_cpu_ns | 10 | 1.022 [0.952, 1.099] | False |
| native | 0 | mean_wall_ns | 10 | 1.022 [0.952, 1.099] | False |
| native | 0 | p99_wall_ns | 10 | 1.053 [0.858, 1.287] | False |
| native | 1 | mean_caller_cpu_ns | 10 | 1.013 [0.932, 1.115] | False |
| native | 1 | mean_wall_ns | 10 | 1.013 [0.932, 1.115] | False |
| native | 1 | p99_wall_ns | 10 | 1.004 [0.745, 1.336] | False |
| lv2 | 0 | mean_caller_cpu_ns | 10 | 0.986 [0.960, 1.014] | False |
| lv2 | 0 | mean_wall_ns | 10 | 0.986 [0.959, 1.014] | False |
| lv2 | 0 | p99_wall_ns | 10 | 0.902 [0.760, 1.048] | False |
| lv2 | 1 | mean_caller_cpu_ns | 10 | 0.966 [0.931, 0.997] | False |
| lv2 | 1 | mean_wall_ns | 10 | 0.966 [0.931, 0.997] | False |
| lv2 | 1 | p99_wall_ns | 10 | 0.862 [0.749, 0.969] | False |
