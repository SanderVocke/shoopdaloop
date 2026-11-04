# Realtime tail clearing

Published `oxisynth-chorus` 0.1.0 from crates.io, original archive SHA-256
`755d6ea336fc37043b00a1b54198e24cb4537c509435c7aca2d837d36185703b`. Original license/manifest
and source are retained. The only addition is `clear_realtime`, which clears
existing delay storage without allocation or rebuilding modulation tables.
Existing synthesis and reset APIs are unchanged.
