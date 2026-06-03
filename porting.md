# Long-term Rust migration plan: bridge-owned object graph

## Motivation

The backend is being migrated from a C++ object graph based on `shoop_shared_ptr`, `shoop_weak_ptr`, and `shoop_enable_shared_from_this` toward Rust-owned implementation. The main recurring difficulty is not any single class. It is preserving lifetime safety while C++ and Rust both need to refer to objects during the transition.

Current intermediate Rust code often stores opaque C++ pointers as `usize` and C++ compensates with ad hoc keepalive containers such as handle-keyed `shared_ptr` maps. That works locally but does not scale: the same pattern will recur for processors, ports, loops, graph objects, sessions, channels, MIDI helpers, and driver back-references.

The migration should therefore introduce a reusable bridge ownership system: a CXX-compatible object handle layer that can represent strong and weak references across the C++/Rust boundary while preserving C++-like keepalive semantics during the migration.

## Long-term target

The long-term target is a thin C++ API/ABI shell over Rust-owned backend logic. During migration, both C++-owned and Rust-owned objects must coexist. The bridge object system is a temporary but central migration scaffold.

The desired steady intermediate model is:

- C++ objects that still exist can be registered into a typed bridge registry.
- Rust stores typed strong/weak handles instead of raw pointers.
- Rust upgrades weak handles before asking C++ trampolines to operate on C++ objects.
- Strong handles keep C++ objects alive, weak handles do not.
- Stale weak handles fail cleanly instead of becoming dangling raw pointers.
- C++ keepalive maps are replaced by explicit bridge strong handles.
- As implementations move to Rust, the same external handle concepts can be backed by Rust-owned objects.

The final model, after the C++ layer is thin enough, should remove this migration scaffold and use idiomatic Rust ownership directly inside the backend.

## Proposed bridge object system

The first version should be pragmatic and C++-owned, because today most objects are still C++ `shared_ptr` objects.

At the non-template CXX boundary, expose simple handle structs:

```cpp
struct BridgeStrongHandle {
    uint64_t id;
    uint32_t type_id;
};

struct BridgeWeakHandle {
    uint64_t id;
    uint32_t type_id;
};
```

The C++ side should provide typed wrappers such as:

```cpp
template<typename T>
class BridgeShared;

template<typename T>
class BridgeWeak;
```

Those wrappers are not exposed directly through CXX. They wrap a non-template registry internally.

Expected operations:

- register a `shoop_shared_ptr<T>` and receive a strong handle
- clone/release strong handles
- downgrade strong to weak
- upgrade weak to optional strong
- look up a strong handle as a typed `shoop_shared_ptr<T>` on the C++ side
- validate object type using `type_id`

Rust should generally store `BridgeWeakHandle` or `BridgeStrongHandle`, never raw C++ object pointers as lifetime authority.

## Audio-thread considerations

The backend has audio-thread/process-thread paths. The bridge registry must document which operations are allowed from those paths.

The first version can prioritize migration safety over perfect real-time behavior, because current code already uses some shared pointer and map operations in process-related paths. However, the design should avoid making things worse where practical.

Guidelines:

- Avoid allocation in hot process paths once handles are registered.
- Prefer handle lookup/upgrade only in process-cycle paths that already tolerate current bookkeeping.
- Avoid blocking waits from the process thread.
- Consider deferred destruction if object teardown might run heavy destructors.
- Add stress tests for unregister/destroy while processing.

## Migration order

The bridge ownership system should be introduced incrementally, proving each step against existing tests.

### Stage 1: bridge handles for processors

IProcessor dispatch is the smallest useful proof. Current Rust `AudioMidiDriverCore` stores processor raw pointers and C++ separately keeps weak refs and pointer-to-handle maps. Replace raw pointers with bridge weak handles.

This proves:

- C++ shared object registration
- Rust weak handle storage
- weak upgrade during process dispatch
- safe stale handle behavior
- C++ trampoline dispatch through typed strong handles

### Stage 2: bridge handles for decoupled MIDI ports

Decoupled MIDI ports are the next target because they currently require explicit C++ keepalive maps while Rust owns queue state. Replace raw decoupled pointers and ad hoc keepalive with bridge handles.

This proves:

- deliberate strong keepalive while registered
- weak/strong handle use for process and close dispatch
- safe unregister and stale handle behavior
- a path toward moving `DecoupledMidiPort` behavior further into Rust

### Stage 3: bridge handles for ports and driver back-references

After processors and decoupled MIDI ports, migrate `MidiPort`, `RustAudioPortF32`, and driver back-references away from raw pointer assumptions where Rust is involved.

### Stage 4: replace C++ bridge registry internals with Rust ownership where appropriate

Once enough objects are Rust-implemented, the registry can evolve to support Rust-owned payloads behind the same handle shape. This should be done only after the C++ lifetime edge cases are well covered by tests.

### Stage 5: remove the migration scaffold

When the C++ layer is thin enough and the internal backend object graph is Rust-native, remove the bridge registry and use ordinary Rust `Arc`, `Weak`, ownership, and lifetimes internally. Keep only the public C/C++ ABI wrappers needed externally.

## Success criteria for the long-term direction

- Rust no longer stores raw C++ pointers as lifetime-authoritative object references.
- Ad hoc C++ keepalive maps disappear in favor of explicit bridge strong handles.
- Weak references fail safely across C++/Rust boundaries.
- Each migration step reduces duplicated lifetime glue.
- Tests cover stale weak handles, unregister during processing, and destruction ordering.
- The bridge layer remains clearly documented as temporary migration infrastructure.
