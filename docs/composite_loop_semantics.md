# Composite Loop Control Semantics

Composite schedules may reference primitive loops or other composites. References must form an acyclic graph, but different composites may share a target.

## Child playback modes

A regular-composite event has one symbolic mode, `DefaultPlayback`. It does not inherit a concrete mode from the composite and does not store a track preference in its schedule. When an inactive primitive child is activated or actually retriggered, the engine resolves `DefaultPlayback` from that child's owning track. The resolved mode stays latched for that active occurrence; changing the track preference affects the next activation without replacing or reconfiguring the composite plan.

A nested composite resolves `DefaultPlayback` as ordinary composite playback and recursively applies its own schedule at the same sample. Regular composites expose only ordinary playback and stop at their outer boundary.

Every script-composite event stores an explicit concrete mode. Explicit script modes bypass primitive track defaults. Explicit playback of a nested regular composite starts that composite normally, after which the nested regular schedule resolves its own `DefaultPlayback` events. Because regular composites expose only ordinary playback and stop, script events requesting recording, replacing, or dry-through-wet modes from a nested regular composite are rejected when the composite graph is configured rather than faulting during playback.

## Explicit control

Starting a composite or seeking it to an iteration establishes an authoritative snapshot at that boundary. Every referenced target desired at the selected iteration receives its scheduled mode and offset. Every other referenced target receives a stop, even if the composite did not previously start it.

A delayed transition applies the same snapshot when its countdown expires. Stopping a composite stops every referenced target. Operations directed at nested composites propagate to their descendants at the same sample.

Authoritative describes the intents emitted at that boundary; it does not give the composite continuing exclusive ownership of shared targets. Every emitted operation participates in normal conflict resolution.

## Schedule advancement

Natural iteration advancement, regular wraparound, script completion, and plan replacement reconcile incrementally. They change targets previously active in that composite or newly desired by the schedule, without repeatedly stopping every referenced inactive target.

## Conflicts

Coincident incompatible operations use this precedence, from highest to lowest:

1. direct control;
2. script composite;
3. regular composite;
4. natural event.

Later accepted direct controls win within their class. Composite and natural ties use stable runtime source identity and action order for deterministic processing. Runtime identity is not persisted user-defined priority and must not be used to encode session behavior.
