# Exceptional blocking waits remain

## Issue

Most ordinary reads use state mirrors, but several engine workflows still require an explicit response or barrier. These paths wait for process-thread progress and therefore retain blocking or rendezvous behavior.

## Affected behavior

- The graph scheduler waits for a topology description before preparing a schedule.
- Schedule installation waits for ownership of the displaced schedule to return so that it is not destroyed on the process thread.
- Command-sequence fences wait for a specific mutation ordering point.
- Driver-settling, queue-drain, and graph-flush operations wait for queued work to complete.
- Some file-load delivery paths block while prepared data is handed to the backend object thread.

## Impact

Each wait depends on another thread or processing cycle making progress. Graph rebuilds consume command capacity and can converge slowly when the engine is busy. If these exceptional paths are called from periodic frontend polling or another latency-sensitive context, they can stall that caller and reintroduce cycle-dependent UI behavior.
