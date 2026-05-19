# Plan: Extract DummyPortCore from DummyPort

## Goal

Refactor `DummyPort` from an inheritance-based base class into a composable struct (`DummyPortCore`), eliminating the diamond MI in `DummyAudioPort` and `DummyMidiPort` and making future Rust porting cleaner.

---

## Current state

```
DummyAudioPort : virtual RustAudioPortF32, DummyPort, WithCommandQueue
                      │                      │
                      └──── virtual PortInterface ────┘  ← diamond

DummyMidiPort : MidiPort, DummyPort, MidiReadableBuffer, MidiWriteableBuffer, WithCommandQueue
                        │               │
                        └─ virtual PortInterface ──┘  ← diamond

DummyPort : virtual PortInterface
  └── holds: m_name, m_direction, m_external_connections
  └── provides: name(), close(), get_external_connection_status(),
                connect_external(), disconnect_external(), maybe_driver_handle()
```

`DummyExternalConnections` (defined in DummyPort.h, implemented in DummyAudioMidiDriver.cpp) stores `DummyPort*` raw pointers for external connection tracking.

---

## Target state

```
DummyPortCore (plain struct, no inheritance)
  └── holds: name, direction, external_connections, dummy_driver_handle
  └── provides: helper methods for all DummyPort behavior

DummyAudioPort : PortInterface (no longer inherits DummyPort or WithCommandQueue)
  └── m_rust_audio_port  (RustAudioPortF32)          ← was base class
  └── m_dummy_port_core  (DummyPortCore)             ← was DummyPort base
  └── m_command_queue    (CommandQueue)              ← was WithCommandQueue mixin

DummyMidiPort : PortInterface (no longer inherits DummyPort or WithCommandQueue)
  └── m_midi_port        (MidiPort)                  ← was base class
  └── m_dummy_port_core  (DummyPortCore)             ← was DummyPort base
  └── m_rust             (rust::Box<DummyMidiPort>)  ← stays
  └── m_command_queue    (CommandQueue)              ← was WithCommandQueue mixin
```

---

## Steps

### Step 1: Create `DummyPortCore` struct

**File:** `src/backend/internal/DummyPort.h` (+ `.cpp`)

- Define `struct DummyPortCore` with:
  - `std::string m_name`
  - `shoop_port_direction_t m_direction`
  - `shoop_weak_ptr<DummyExternalConnections> m_external_connections`
  - `void* m_dummy_driver_handle` (to preserve `maybe_driver_handle()` returning `(void*)this` behavior)
- Move all `DummyPort` method implementations into `DummyPortCore` methods that take a `DummyPortCore& self` (or make them member functions on the struct).
- The `DummyPort` class stays temporarily as a thin adapter that holds a `DummyPortCore` and forwards — or we remove it entirely if no other code directly uses `DummyPort` as a type.
- **Key detail:** `DummyExternalConnections::connect()`, `disconnect()`, and `connection_status_of()` take `DummyPort*`. Change these to take `DummyPortCore*` (or `void*` + name string). This means updating the `m_external_connections` vector in `DummyExternalConnections` to store `DummyPortCore*` instead of `DummyPort*`.

### Step 2: Update `DummyExternalConnections` to use `DummyPortCore*`

**Files:** `src/backend/internal/DummyPort.h`, `src/backend/internal/DummyAudioMidiDriver.cpp`

- Change `std::vector<std::pair<DummyPort*, std::string>> m_external_connections` → `std::vector<std::pair<DummyPortCore*, std::string>> m_external_connections`
- Update `connect()`, `disconnect()`, `connection_status_of()`, `get_port()` signatures accordingly.
- All call sites in `DummyPortCore` methods and `DummyAudioMidiDriver` methods updated.

### Step 3: Refactor `DummyAudioPort` to use composition

**Files:** `src/backend/internal/DummyAudioPort.h`, `src/backend/internal/DummyAudioPort.cpp`

- Remove `DummyPort` and `WithCommandQueue` from inheritance list.
- Add members:
  - `DummyPortCore m_dummy_port_core;`
  - `CommandQueue m_command_queue;`
- Change `PortInterface` to non-virtual inheritance (it's now the only base).
- Replace all `DummyPort::close()`, `DummyPort::name()`, etc. calls with `m_dummy_port_core.*()` equivalents.
- Replace `exec_process_thread_command()`, `PROC_handle_command_queue()` calls to use `m_command_queue.*()` directly.
- Constructor initializer: initialize `m_dummy_port_core` with name, direction, external connections.
- Update `test_DummyPorts.cpp` and `test_DummyAudioMidiDriver.cpp` — should compile unchanged if the public API is preserved.

### Step 4: Refactor `DummyMidiPort` to use composition

**Files:** `src/backend/internal/DummyMidiPort.h`, `src/backend/internal/DummyMidiPort.cpp`

- Remove `DummyPort` and `WithCommandQueue` from inheritance list.
- Keep `MidiPort` as base (needed for graph integration and MidiPort interface).
- Add members:
  - `DummyPortCore m_dummy_port_core;`
  - `CommandQueue m_command_queue;`
- `MidiPort` already inherits `virtual PortInterface`, and we now also implement `PortInterface` methods directly. Use `using MidiPort::...` or explicit overrides to resolve ambiguity.
- Replace `DummyPort::close()`, `DummyPort::name()`, etc. with `m_dummy_port_core.*()`.
- Replace `exec_process_thread_command()`, `PROC_handle_command_queue()` with `m_command_queue.*()`.
- The comment in DummyMidiPort.h says "PortInterface methods (from DummyPort base, but need explicit override due to MidiPort also having these)" — after this change, these overrides just delegate to `m_dummy_port_core` + `MidiPort` as needed.

### Step 5: Update `DummyAudioMidiDriver`

**Files:** `src/backend/internal/DummyAudioMidiDriver.h`, `src/backend/internal/DummyAudioMidiDriver.cpp`

- Update references to `DummyPort*` → `DummyPortCore*` in any internal code (if any exist).
- The `open_audio_port()` and `open_midi_port()` methods construct `DummyAudioPort` / `DummyMidiPort` — constructor signatures may change slightly.
- `add_external_mock_port()`, `remove_external_mock_port()`, etc. delegate to `m_external_connections` — signatures updated in step 2.

### Step 6: Build and test

```bash
# 1. Build everything (C++ + Rust)
cargo build

# 2. Run Rust unit tests
cargo test

# 3. Run C++ unit/integration tests
# The test_runner binary is produced as a side effect of building the backend crate.
# Find and run it:
find target -name "test_runner" -type f -executable | head -1 | xargs ./

# 4. If final commit, format and rebuild with warnings as errors:
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

---

## Files to modify

| File | Change |
|---|---|
| `src/backend/internal/DummyPort.h` | Add `DummyPortCore` struct, update `DummyExternalConnections` to use `DummyPortCore*` |
| `src/backend/internal/DummyPort.cpp` | Move implementations to `DummyPortCore` methods |
| `src/backend/internal/DummyAudioPort.h` | Remove `DummyPort`/`WithCommandQueue` inheritance, add composition members |
| `src/backend/internal/DummyAudioPort.cpp` | Update ctor, destructor, method implementations |
| `src/backend/internal/DummyMidiPort.h` | Remove `DummyPort`/`WithCommandQueue` inheritance, add composition members |
| `src/backend/internal/DummyMidiPort.cpp` | Update ctor, destructor, method implementations |
| `src/backend/internal/DummyAudioMidiDriver.h` | Minor: update types if needed |
| `src/backend/internal/DummyAudioMidiDriver.cpp` | Update `DummyExternalConnections` method implementations |

## Files NOT to modify (public API preserved)

| File | Reason |
|---|---|
| `src/backend/test/unit/test_DummyPorts.cpp` | Public API unchanged — tests should pass as-is |
| `src/backend/test/unit/test_DummyAudioMidiDriver.cpp` | Public API unchanged |
| `src/backend/test/unit/test_DummyAudioMidiDriver.cpp` | Public API unchanged |
| Rust modules (`dummy_midi_port.rs`, etc.) | No CXX bridge surface changes |

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| `DummyExternalConnections::connection_status_of()` returns `PortExternalConnectionStatus` (map of name→bool). It compares `conn.first == port` where `port` is a `DummyPort*`. Changing to `DummyPortCore*` requires updating the comparison. | Update the vector type and comparison. The pointer identity semantics are preserved since we still compare the same object's address. |
| `DummyPort::maybe_driver_handle()` returns `(void*)this`. With composition, the `DummyPortCore` address differs from the `DummyAudioPort` address. | Store an explicit `void* m_dummy_driver_handle` in `DummyPortCore`, or return the outer object's `this` via a back-reference. Since the dummy driver handle is only used for opaque identification, storing a dedicated handle pointer is simplest. |
| `DummyMidiPort` inherits both `MidiPort` (which inherits `PortInterface`) and now directly implements `PortInterface`. | Use `override` on the concrete methods to resolve. `MidiPort` provides default implementations; `DummyMidiPort` overrides where it needs custom behavior (delegating to `m_dummy_port_core`). The `using` directive may help disambiguate. |
| `WithCommandQueue` has a configurable constructor (`size`, `timeout_ms`, `poll_interval_us`). `DummyAudioPort` uses `(100)`, `DummyMidiPort` uses `(100, 1000, 1000)`. | Initialize `m_command_queue` in the constructor with the same parameters. `CommandQueue` is already a standalone class. |
