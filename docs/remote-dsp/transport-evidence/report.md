# Matched audio-transport measurements

102 of 102 expected records; 152,640,000 verified samples; 0 incorrect; 0 unverified.

All cases use yielding, catch-up without intentional overdue-source skips, identical source deadlines and DSP wire frames. UDP-gap retains the 200 us gap policy; UDP-complete and TCP-complete only process complete input blocks. TCP uses a separate authenticated, length-framed connection with TCP_NODELAY at both ends. Audio latency is ((N+1)*P+A)/fs, excluding audio hardware and extra endpoint buffers. Counts are output misses / concealed input; input counts include warm-up. No clock synchronization is needed.

## Fixed long comparisons

| P/k/N | Audio ms | Scheduling | UDP-gap | UDP-complete | TCP-complete |
| --- | ---: | --- | ---: | ---: | ---: |
| 8/1/4 | 0.833 | other | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/4 | 0.833 | fifo1 | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/8 | 1.500 | other | 0 / 0 | 0 / 0 | 0 / 0 |
| 8/1/8 | 1.500 | fifo1 | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/4 | 1.667 | other | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/4 | 1.667 | fifo1 | 0 / 0 | 0 / 0 | 0 / 0 |
| 16/4/8 | 3.000 | other | 0 / 1 | 0 / 0 | 0 / 0 |
| 16/4/8 | 3.000 | fifo1 | 0 / 0 | 0 / 0 | 0 / 0 |

## Every condition, with all repetitions retained

| Suite | P/k/N | Variant | Scheduling | Runs | Misses / slots | Concealed | Retired | Healthy |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| followup | 8/1/4 | tcp-complete | fifo1 | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/4 | tcp-complete | other | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/4 | udp-complete | fifo1 | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/4 | udp-complete | other | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/4 | udp-gap | fifo1 | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/4 | udp-gap | other | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/8 | tcp-complete | fifo1 | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/8 | tcp-complete | other | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/8 | udp-complete | fifo1 | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/8 | udp-complete | other | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/8 | udp-gap | fifo1 | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 8/1/8 | udp-gap | other | 1 | 0 / 360,000 | 0 | 0 | 1 |
| followup | 16/4/4 | tcp-complete | fifo1 | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/4 | tcp-complete | other | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/4 | udp-complete | fifo1 | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/4 | udp-complete | other | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/4 | udp-gap | fifo1 | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/4 | udp-gap | other | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/8 | tcp-complete | fifo1 | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/8 | tcp-complete | other | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/8 | udp-complete | fifo1 | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/8 | udp-complete | other | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/8 | udp-gap | fifo1 | 1 | 0 / 180,000 | 0 | 0 | 1 |
| followup | 16/4/8 | udp-gap | other | 1 | 0 / 180,000 | 1 | 0 | 0 |
| screen | 8/1/4 | tcp-complete | fifo1 | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/4 | tcp-complete | other | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/4 | udp-complete | fifo1 | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/4 | udp-complete | other | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/4 | udp-gap | fifo1 | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/4 | udp-gap | other | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/8 | tcp-complete | fifo1 | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/8 | tcp-complete | other | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/8 | udp-complete | fifo1 | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/8 | udp-complete | other | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/8 | udp-gap | fifo1 | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 8/1/8 | udp-gap | other | 3 | 0 / 36,000 | 0 | 0 | 3 |
| screen | 16/4/4 | tcp-complete | fifo1 | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/4 | tcp-complete | other | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/4 | udp-complete | fifo1 | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/4 | udp-complete | other | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/4 | udp-gap | fifo1 | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/4 | udp-gap | other | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/8 | tcp-complete | fifo1 | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/8 | tcp-complete | other | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/8 | udp-complete | fifo1 | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/8 | udp-complete | other | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/8 | udp-gap | fifo1 | 3 | 0 / 18,000 | 0 | 0 | 3 |
| screen | 16/4/8 | udp-gap | other | 3 | 0 / 18,000 | 0 | 0 | 3 |
| stall | 16/4/8 | tcp-complete | fifo1 | 1 | 0 / 3,000 | 0 | 0 | 1 |
| stall | 16/4/8 | tcp-complete | other | 1 | 0 / 3,000 | 0 | 0 | 1 |
| stall | 16/4/8 | udp-complete | fifo1 | 1 | 0 / 3,000 | 0 | 0 | 1 |
| stall | 16/4/8 | udp-complete | other | 1 | 0 / 3,000 | 0 | 0 | 1 |
| stall | 16/4/8 | udp-gap | fifo1 | 1 | 0 / 3,000 | 3 | 0 | 0 |
| stall | 16/4/8 | udp-gap | other | 1 | 0 / 3,000 | 3 | 0 | 0 |

The manifest preserves both endpoint inventories, binary/source identities, permissions and expected cases. The archive retains every outcome, histogram, native per-thread scheduling observation, socket/queue counter, command and timestamp. Child CPU cost covers both processes locally; over SSH it excludes the remote server and is explicitly labeled. Host network counters include unrelated traffic; these are not per-stream packet-loss measurements.
