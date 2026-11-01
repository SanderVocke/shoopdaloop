# Recording-capacity diagnostic

The Windows release stress test at `2dcf153e` ran 62.72 seconds and observed
`Stopped` where it expected `Recording`. Its host had an explicitly reduced
16 MiB recording pool. The full run is
[41309817206](https://github.com/SanderVocke/shoopdaloop/actions/runs/41309817206);
the remaining Node job was stopped to obtain the completed failure log archive.

The [probe source](recording-capacity-probe.rs) uses the public native driver and
engine protocol, the same 8-track/80-loop/two-channel pool description, a mono
strip and nonzero dummy input. One connected client starts recording and never
disconnects or sends a stop. Controlled native-driver callbacks advance 120
seconds at 48 kHz in 256-frame quanta. Production sources are pinned at `2dcf153e`
(identical to the passing `929ed71b` production sources).

[Recorded output](recording-capacity-probe.log), Linux x86_64, Rust 1.99.1, release:

| Prepared budget | Result after 120 rendered seconds |
| --- | --- |
| 16 MiB / 4096 pages of 4096 bytes | Recording stops after 2,096,384 frames, about 43.675 seconds, without any UI loss. |
| 256 MiB / 65,536 pages of 4096 bytes | Still recording; no stop observed. |

The stress fixture now uses the application's normal 256 MiB default, preserving
48 kHz, all 32 detach/attach cycles, every recording/identity/progress assertion,
operation/result bounds and descriptor reclamation checks. Other host fixtures
keep their 16 MiB budgets. Production limits, exhaustion behavior, deadlines and
resource-exhaustion tests are unchanged. The probe is control-side diagnostic
evidence, not an additional heap-safety or wall-clock performance claim.

To replay from the repository root:

```sh
mkdir -p work/recording-capacity-probe/src
cp docs/native_session_recovery/evidence/recording-capacity-probe.rs work/recording-capacity-probe/src/main.rs
cp docs/native_session_recovery/evidence/recording-capacity-probe.toml work/recording-capacity-probe/Cargo.toml
CARGO_TARGET_DIR=target cargo run --release --manifest-path work/recording-capacity-probe/Cargo.toml
```
