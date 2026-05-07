# Double-Bridge Architecture for Carla LV2 Processing Chain

## Goal

Insert two cxx bridges between C++ consumers and the existing
`CarlaLV2ProcessingChain` implementation:

1. **Inner bridge (temporary)** — Rust → C++  
   Wraps the C++ `CarlaLV2ProcessingChain` in a non-template facade and
   exposes it to Rust via `extern "C++"`.

2. **Outer bridge (permanent)** — Rust → C++  
   Exposes a Rust `FxChain` type to C++ via `extern "Rust"`.  C++ consumers
   talk *only* to this bridge.  Today the Rust `FxChain` delegates to the
   inner bridge; after the port it will contain a pure-Rust implementation.

Both bridges live in one new Cargo crate (`src/rust/carla_lv2`) so that
opaque C++ types declared in the inner bridge can flow through to the
outer bridge without needing to cross crate boundaries.

```
            ┌──────────────────────────────────────────────┐
            │  Rust crate: carla_lv2                       │
            │                                              │
   outer    │   FxChain (Rust struct)                      │
   bridge   │   delegates every call to cpp_facade          │
◄─ C++ ────►──────────────────────────────────────────────►│
            │         │                                    │
            │         │  inner bridge (extern "C++")        │
            │         ▼                                    │
            │   CarlaLv2ChainCppFacade                     │
            │   (UniquePtr<…> held inside FxChain)         │
            │                                              │
            └─────────────────┬────────────────────────────┘
                              │  calls into
                              ▼
                 C++  CarlaLV2ProcessingChain
                 (unchanged, keeps owning ports)
```

## Crate layout

```
src/rust/carla_lv2/
├── Cargo.toml
├── build.rs               # runs cxx_build::bridge on both bridge modules
├── include/
│   └── CarlaLv2ChainCppFacade.h    # C++ facade class used by inner bridge
└── src/
    ├── lib.rs              # crate root, re-exports both bridge modules
    ├── inner_bridge.rs     # #[cxx::bridge] – extern "C++"
    ├── outer_bridge.rs     # #[cxx::bridge] – extern "Rust"
    └── fx_chain.rs         # Rust FxChain impl delegating to inner bridge
```

New C++ files in the existing backend tree:

```
src/backend/internal/lv2/
├── CarlaLv2ChainCppFacade.h    # non-template wrapper around CarlaLV2ProcessingChain
├── CarlaLv2ChainCppFacade.cpp
├── RustFxChainAdapter.h        # ProcessingChainInterface adapter – permanent C++ owner
└── RustFxChainAdapter.cpp
```

---

## 1. Inner bridge — Rust calls C++

### 1.1  C++ facade class

`CarlaLv2ChainCppFacade` is a **non-template** wrapper around the concrete
`CarlaLV2ProcessingChain<uint32_t, uint16_t>`.  All its methods are plain
return types that cxx can handle (`u32`, `bool`, `String`, `usize`, …).

```cpp
// CarlaLv2ChainCppFacade.h
#pragma once
#include "CarlaLV2ProcessingChain.h"  // the existing template class
#include <memory>
#include <string>
#include <cstdint>

namespace carla_lv2_inner {

class CarlaLv2ChainCppFacade {
public:
    using Time  = uint32_t;
    using Size  = uint16_t;
    using Chain = CarlaLV2ProcessingChain<Time, Size>;

    // constructors / factory
    static std::unique_ptr<CarlaLv2ChainCppFacade> create(
        uint32_t chain_type,      // shoop_fx_chain_type_t as u32
        uint32_t sample_rate,
        uint32_t buffer_size,
        std::string  title);

    // control
    void process(uint32_t frames);
    bool is_ready() const;
    bool is_active() const;
    void set_active(bool active);
    void show();
    void hide();
    bool visible() const;
    void stop();

    // state
    std::string serialize_state(uint32_t timeout_ms) const;
    void        deserialize_state(std::string state);

    // port counts (needed by C++ adapter to size vectors)
    uint32_t n_audio_input_ports() const;
    uint32_t n_audio_output_ports() const;
    uint32_t n_midi_input_ports() const;

    // port pointers as raw uintptr_t so cxx can pass them as usize
    // The caller must cast back to InternalAudioPort<float>* / MidiPort*
    // Lifetime: pointers stay valid until this facade is destroyed.
    uintptr_t audio_input_port_ptr(uint32_t idx) const;
    uintptr_t audio_output_port_ptr(uint32_t idx) const;
    uintptr_t midi_input_port_ptr(uint32_t idx) const;

private:
    shoop_shared_ptr<Chain> m_chain;
    explicit CarlaLv2ChainCppFacade(shoop_shared_ptr<Chain> chain);
};

} // namespace carla_lv2_inner
```

**Important rules for the facade:**
- Belongs to the `carla_lv2_inner` namespace (matches the cxx `namespace`).
- No template methods, no function overloads, no reference arguments
  (cxx restricts those).
- Returns `std::unique_ptr` so cxx can transfer ownership into Rust's
  `UniquePtr`.
- The `create` factory internally uses `LV2::create_carla_chain` to produce
  the real `CarlaLV2ProcessingChain` and wraps it in a `shoop_shared_ptr`.
- Port pointers are returned as `uintptr_t` (`usize` on the Rust side);
  the C++ adapter casts them back to the real types.

### 1.2  Inner bridge definition  (`src/inner_bridge.rs`)

```rust
#[cxx::bridge(namespace = "carla_lv2_inner")]
pub mod ffi {
    extern "C++" {
        include!("CarlaLv2ChainCppFacade.h");

        type CarlaLv2ChainCppFacade;

        fn create(
            chain_type:   u32,
            sample_rate:  u32,
            buffer_size:  u32,
            title:        String,
        ) -> Result<UniquePtr<CarlaLv2ChainCppFacade>>;

        fn process(self: &CarlaLv2ChainCppFacade, frames: u32);
        fn is_ready(self: &CarlaLv2ChainCppFacade) -> bool;
        fn is_active(self: &CarlaLv2ChainCppFacade) -> bool;
        fn set_active(self: &CarlaLv2ChainCppFacade, active: bool);
        fn show(self: &CarlaLv2ChainCppFacade);
        fn hide(self: &CarlaLv2ChainCppFacade);
        fn visible(self: &CarlaLv2ChainCppFacade) -> bool;
        fn stop(self: &mut CarlaLv2ChainCppFacade);

        fn serialize_state(self: &CarlaLv2ChainCppFacade, timeout_ms: u32) -> String;
        fn deserialize_state(self: &CarlaLv2ChainCppFacade, state: String);

        fn n_audio_input_ports(self: &CarlaLv2ChainCppFacade) -> u32;
        fn n_audio_output_ports(self: &CarlaLv2ChainCppFacade) -> u32;
        fn n_midi_input_ports(self: &CarlaLv2ChainCppFacade) -> u32;

        fn audio_input_port_ptr(self: &CarlaLv2ChainCppFacade, idx: u32) -> usize;
        fn audio_output_port_ptr(self: &CarlaLv2ChainCppFacade, idx: u32) -> usize;
        fn midi_input_port_ptr(self: &CarlaLv2ChainCppFacade, idx: u32) -> usize;
    }
}
```

`cxx-build` will generate:
- `include/carla_lv2/src/inner_bridge.rs.h` — C++ header with function
  signatures that the facade **must** implement exactly.
- `src/inner_bridge.rs.h` (hidden) — the Rust-side glue.

---

## 2. Outer bridge — C++ calls Rust

### 2.1  Rust `FxChain` struct  (`src/fx_chain.rs`)

```rust
use crate::inner_bridge::ffi as inner;

pub struct FxChain {
    cpp_facade: cxx::UniquePtr<inner::CarlaLv2ChainCppFacade>,
}

impl FxChain {
    pub fn new(
        chain_type:  u32,
        sample_rate: u32,
        buffer_size: u32,
        title:       String,
    ) -> Result<Box<Self>, Box<dyn std::error::Error>> {
        let cpp_facade = inner::create(chain_type, sample_rate, buffer_size, title)?;
        Ok(Box::new(FxChain { cpp_facade }))
    }
}

// Every method on the outer-bridge type just delegates to the C++
// facade one-for-one.
impl FxChain {
    fn process(&self, frames: u32)         { self.cpp_facade.process(frames); }
    fn is_ready(&self) -> bool             { self.cpp_facade.is_ready() }
    fn is_active(&self) -> bool            { self.cpp_facade.is_active() }
    fn set_active(&self, active: bool)     { self.cpp_facade.set_active(active); }
    fn show(&self)                         { self.cpp_facade.show(); }
    fn hide(&self)                         { self.cpp_facade.hide(); }
    fn visible(&self) -> bool              { self.cpp_facade.visible() }
    fn stop(&mut self)                     { self.cpp_facade.as_mut().unwrap().stop(); }
    fn serialize_state(&self, ms: u32) -> String { self.cpp_facade.serialize_state(ms) }
    fn deserialize_state(&self, s: String) { self.cpp_facade.deserialize_state(s); }

    fn n_audio_input_ports(&self) -> u32   { self.cpp_facade.n_audio_input_ports() }
    fn n_audio_output_ports(&self) -> u32  { self.cpp_facade.n_audio_output_ports() }
    fn n_midi_input_ports(&self) -> u32    { self.cpp_facade.n_midi_input_ports() }
    fn audio_input_port_ptr(&self, i: u32) -> usize  { self.cpp_facade.audio_input_port_ptr(i) }
    fn audio_output_port_ptr(&self, i: u32) -> usize { self.cpp_facade.audio_output_port_ptr(i) }
    fn midi_input_port_ptr(&self, i: u32) -> usize   { self.cpp_facade.midi_input_port_ptr(i) }
}
```

### 2.2  Outer bridge definition  (`src/outer_bridge.rs`)

```rust
#[cxx::bridge(namespace = "carla_lv2")]
pub mod ffi {
    extern "Rust" {
        type FxChain;

        fn create_fx_chain(
            chain_type:  u32,
            sample_rate: u32,
            buffer_size: u32,
            title:       String,
        ) -> Result<Box<FxChain>>;

        fn process(self: &FxChain, frames: u32);
        fn is_ready(self: &FxChain) -> bool;
        fn is_active(self: &FxChain) -> bool;
        fn set_active(self: &FxChain, active: bool);
        fn show(self: &FxChain);
        fn hide(self: &FxChain);
        fn visible(self: &FxChain) -> bool;
        fn stop(self: &mut FxChain);
        fn serialize_state(self: &FxChain, timeout_ms: u32) -> String;
        fn deserialize_state(self: &FxChain, state: String);

        fn n_audio_input_ports(self: &FxChain) -> u32;
        fn n_audio_output_ports(self: &FxChain) -> u32;
        fn n_midi_input_ports(self: &FxChain) -> u32;
        fn audio_input_port_ptr(self: &FxChain, idx: u32) -> usize;
        fn audio_output_port_ptr(self: &FxChain, idx: u32) -> usize;
        fn midi_input_port_ptr(self: &FxChain, idx: u32) -> usize;
    }
}
```

`cxx-build` will generate `include/carla_lv2/src/outer_bridge.rs.h` which C++
can `#include` to get the `carla_lv2::` C++ namespace.

---

## 3. C++ adapter — plugs into the existing graph

`RustFxChainAdapter` is a concrete C++ class that inherits from the three
interfaces `ProcessingChainInterface<Time,Size>`, `ExternalUIInterface` and
`SerializeableStateInterface`.  It is the **only** type the existing code
sees — `BackendSession::create_fx_chain` will construct it instead of
`CarlaLV2ProcessingChain`.

```cpp
// RustFxChainAdapter.h
#pragma once
#include "ProcessingChainInterface.h"
#include "ExternalUIInterface.h"
#include "SerializeableStateInterface.h"
#include "carla_lv2/src/outer_bridge.rs.h"   // generated by cxx-build

#include <vector>
#include <cstdint>

class RustFxChainAdapter
    : public ProcessingChainInterface<uint32_t, uint16_t>,
      public ExternalUIInterface,
      public SerializeableStateInterface {
public:
    using Time = uint32_t;
    using Size = uint16_t;

    explicit RustFxChainAdapter(
        rust::Box<carla_lv2::FxChain> chain);

    // --- ProcessingChainInterface ---
    std::vector<SharedInternalAudioPort> const& input_audio_ports()  const override;
    std::vector<SharedInternalAudioPort> const& output_audio_ports() const override;
    std::vector<SharedMidiPort>         const& input_midi_ports()    const override;
    void process(uint32_t frames) override;
    bool is_ready()          const override;
    bool is_active()         const override;
    void set_active(bool a) override;
    bool is_freewheeling()   const override { return false; }
    void set_freewheeling(bool) override {}
    void stop() override;

    // --- ExternalUIInterface ---
    bool visible() const override;
    void show() override;
    void hide() override;

    // --- SerializeableStateInterface ---
    std::string serialize_state(uint32_t timeout_ms) override;
    void        deserialize_state(std::string state) override;

    // Needed by GraphFXChain to access ExternalUIInterface &
    // SerializeableStateInterface through the chain pointer
    // (which is typed as ProcessingChainInterface&).
    //   → dynamic_cast when needed.

private:
    rust::Box<carla_lv2::FxChain> m_rust_chain;

    // Cached port collections.
    // The shared_ptrs are built from raw pointers obtained through the
    // outer bridge.  The CARLA-LV2 chain owns the pointees; these
    // shared_ptrs use a no-op deleter so we do not double-free.
    // Lifetime guarantee: m_rust_chain → FxChain → UniquePtr<CppFacade>
    //   → shoop_shared_ptr<Chain> → ports alive until drop.
    std::vector<SharedInternalAudioPort> m_input_audio_ports;
    std::vector<SharedInternalAudioPort> m_output_audio_ports;
    std::vector<SharedMidiPort>         m_input_midi_ports;
};
```

Implementation sketch:

```cpp
// RustFxChainAdapter.cpp
#include "RustFxChainAdapter.h"
#include "InternalAudioPort.h"
#include "MidiPort.h"

RustFxChainAdapter::RustFxChainAdapter(
    rust::Box<carla_lv2::FxChain> chain)
    : m_rust_chain(std::move(chain))
{
    // Audio inputs
    auto n = m_rust_chain->n_audio_input_ports();
    m_input_audio_ports.reserve(n);
    for (uint32_t i = 0; i < n; ++i) {
        auto* ptr = reinterpret_cast<_InternalAudioPort*>(
                        m_rust_chain->audio_input_port_ptr(i));
        m_input_audio_ports.emplace_back(ptr, [](auto*){});
    }
    // Audio outputs – same pattern
    // MIDI inputs – same pattern with MidiPort*
}

void RustFxChainAdapter::process(uint32_t frames) {
    m_rust_chain->process(frames);
}

std::vector<SharedInternalAudioPort> const&
RustFxChainAdapter::input_audio_ports() const { return m_input_audio_ports; }
// ... etc for all methods, each forwarding to m_rust_chain
```

**Port lifetime note:** The ports' real owner is the `CarlaLV2ProcessingChain`
held inside the C++ facade.  The adapter outlives the ports because
`m_rust_chain` is a member and is dropped last.  Using a no-op deleter
shared_ptr is safe here; when the Rust `FxChain` is dropped, it drops the
facade, which drops the shared_ptr to the C++ chain.  Since the adapter's
shared_ptrs have no-op deleters, the ports are freed only once by the real
owner.  Order: adapter's shared_ptrs cleaned first (no-op), then
`m_rust_chain` destructor → real chain destructor → ports freed.

---

## 4. Wiring into C++ code

### 4.1  BackendSession

Currently `BackendSession::create_fx_chain` for Carla types does:

```cpp
shoop_shared_ptr<ProcessingChainInterface<Time, Size>> chain;
switch (type) {
  case Carla_Rack:
  case Carla_Patchbay:
  case Carla_Patchbay_16x:
    chain = lv2.create_carla_chain<Time, Size>(...);
    break;
  // ...
}
auto info = shoop_make_shared<GraphFXChain>(chain, shared_from_this());
```

Change it to:

```cpp
case Carla_Rack:
case Carla_Patchbay:
case Carla_Patchbay_16x:
{
    rust::Box<carla_lv2::FxChain> rust_chain = carla_lv2::create_fx_chain(
        static_cast<uint32_t>(type),
        m_sample_rate,
        m_buffer_size,
        std::string(title));
    chain = shoop_static_pointer_cast<ProcessingChainInterface<Time, Size>>(
        shoop_make_shared<RustFxChainAdapter>(std::move(rust_chain)));
    break;
}
```

The same adapter object also serves as `ExternalUIInterface` and
`SerializeableStateInterface` when the C-API / front-end queries the chain.

The code in `BackendSession.cpp` that does `dynamic_cast<NotifyProcessParametersInterface*>(…)`
already works since the pointers are forwarded through the adapter.

### 4.2  GraphFXChain

**No changes needed.**  It receives a `shoop_shared_ptr<FXChain>` where
`FXChain` is `ProcessingChainInterface<Time,Size>` — the adapter satisfies
this interface exactly.

### 4.3  C API / existing facade

The C API (`libshoopdaloop_backend.h`) and the existing `FXChain` Rust
wrapper (`src/rust/backend_bindings/src/fx_chain.rs`) call the C API
functions.  Those C API functions already work against
`ProcessingChainInterface` and query ports / state / UI through
`dynamic_cast`.  With the adapter, everything continues working — no
changes required.

---

## 5. Build integration

### 5.1  New Cargo crate

`src/rust/carla_lv2/Cargo.toml`:
```toml
[package]
name = "carla_lv2"
links = "carla_lv2"
edition = "2021"

[lib]
crate-type = ["rlib", "staticlib"]

[dependencies]
cxx = { workspace = true }

[build-dependencies]
cxx-build = { workspace = true }

[features]
prebuild = []
```

`src/rust/carla_lv2/build.rs`:
```rust
fn main() {
    if cfg!(feature = "prebuild") { return; }

    // Build both cxx bridges into a single C++ library.
    cxx_build::bridges(&["src/inner_bridge.rs", "src/outer_bridge.rs"])
        .std("c++20")
        .compile("carla_lv2_cxx");

    let out_dir = std::path::PathBuf::from(
        std::env::var("OUT_DIR").unwrap());

    // Export include path for CMake.
    println!(
        "cargo:include={}",
        out_dir.join("cxxbridge/include").display()
    );
    // Export library search path for CMake.
    println!(
        "cargo:cxx_bridge_libdir={}",
        out_dir.display()
    );
    // Export the path to the staticlib for CMake.
    println!(
        "cargo:rust_libdir={}",
        out_dir.join("../../../").display()
    );

    println!("cargo:rerun-if-changed=src/inner_bridge.rs");
    println!("cargo:rerun-if-changed=src/outer_bridge.rs");
    println!("cargo:rerun-if-changed=src/fx_chain.rs");
}
```

### 5.2  Workspace membership

Add `"src/rust/carla_lv2"` to the `members` array in the workspace
`Cargo.toml`.

### 5.3  CMake changes

In `src/backend/CMakeLists.txt`, add (after the `refilling_pool` section):

```cmake
# carla_lv2 CXX bridge
include_directories(${CARLA_LV2_CXX_INCLUDE})
link_directories(${CARLA_LV2_CXX_LIBDIR})

target_link_libraries(shoopdaloop_backend_internals PRIVATE
  ${CARLA_LV2_RUST_LIB}
  carla_lv2_cxx
)
```

Add `CarlaLv2ChainCppFacade.cpp` and `RustFxChainAdapter.cpp` to the
`INTERNAL_SOURCES` glob or list them explicitly (since they are in
different source files).

### 5.4  Propagating CMake variables from Cargo

In the backend `build.rs` (the Rust crate that drives the CMake
build), read the `DEP_CARLA_LV2_INCLUDE`, `DEP_CARLA_LV2_CXX_BRIDGE_LIBDIR`
and `CARGO_STATICLIB_FILE_CARLA_LV2` environment variables and pass
them to CMake:

```rust
let carla_lv2_cxx_include = std::env::var("DEP_CARLA_LV2_INCLUDE")?;
let carla_lv2_cxx_libdir  = std::env::var("DEP_CARLA_LV2_CXX_BRIDGE_LIBDIR")?;
let carla_lv2_staticlib   = std::env::var("CARGO_STATICLIB_FILE_CARLA_LV2")?;

// … pass to cmake_config:
.define("CARLA_LV2_CXX_INCLUDE", carla_lv2_cxx_include)
.define("CARLA_LV2_CXX_LIBDIR",  carla_lv2_cxx_libdir)
.define("CARLA_LV2_RUST_LIB",    carla_lv2_staticlib)
```

### 5.5  Include path for the facade header

The C++ facade header (`CarlaLv2ChainCppFacade.h`) lives in the
existing backend tree (`src/backend/internal/lv2/`), so it is already
visible to the CMake target.  The `include!()` directive in the inner
bridge must reference it by a path that resolves during cxx codegen.
Since cxx runs `cxx_build` which invokes the C++ compiler with custom
include flags, add the backend lv2 directory to the include path in
`build.rs`:

```rust
cc_build = cxx_build::bridges(&[...])
    .include("../../backend/internal/lv2")      // CarlaLv2ChainCppFacade.h
    .include("../../backend/internal")          // transitive includes
    .include("../../backend")                   // types.h etc.
    // plus any LV2 / lilv include dirs from pkg-config
    .std("c++20")
    .compile("carla_lv2_cxx");
```

If lilv and LV2 headers are installed system-wide they will be found
automatically; otherwise the pkg-config paths must also be passed.

---

## 6. C++ adapter changes for GraphFXChain dynamic casts

`GraphFXChain.cpp` uses `dynamic_cast`:

```cpp
void GraphFXChain::graph_node_process(uint32_t nframes) {
    if (chain) { chain->process(nframes); }
}
```

The C API wrappers in the existing codebase do dynamic_casts to
`ExternalUIInterface` and `SerializeableStateInterface`.  Since our
adapter inherits all three interfaces, those dynamic_casts will resolve
correctly.

Verify the following call-sites still compile and pass tests:

| File | Cast |
|------|------|
| `libshoopdaloop_backend.cpp` | `dynamic_cast<ExternalUIInterface*>` |
| `libshoopdaloop_backend.cpp` | `dynamic_cast<SerializeableStateInterface*>` |
| `BackendSession.cpp` | `dynamic_cast<NotifyProcessParametersInterface*>` |

No code changes are needed — the adapter's multiple inheritance means
these casts succeed.

---

## 7. Cxx type-mapping notes

| C++ type | cxx Rust type | Direction |
|----------|---------------|-----------|
| `uint32_t` | `u32` | both |
| `bool` | `bool` | both |
| `std::string` | `String` | both |
| `std::unique_ptr<T>` | `UniquePtr<T>` | C++ → Rust |
| `rust::Box<T>` | `Box<T>` | Rust → C++ |
| `uintptr_t` | `usize` | both (for raw pointers) |
| `shoop_shared_ptr<T>` | **unsupported** | must tunnel as `usize` |

---

## 8. Implementation order

1. Create the `carla_lv2` crate skeleton: `Cargo.toml`, `build.rs`, `src/lib.rs`.
2. Write `CarlaLv2ChainCppFacade.h/.cpp` — pure C++, tested independently.
3. Write the inner bridge (`inner_bridge.rs`) and verify it compiles with
   the facade header.  At this point the bridge generates C++ glue that is
   compiled into `carla_lv2_cxx`.
4. Write the Rust `FxChain` struct, `outer_bridge.rs`.  Verify the
   double bridge compiles end-to-end.
5. Write `RustFxChainAdapter.h/.cpp`.
6. Update `BackendSession::create_fx_chain` to use the adapter.
7. Update CMake: add new sources, link `carla_lv2_cxx`.
8. Update the backend `build.rs` to pass `CARLA_LV2_*` variables.
9. Add `carla_lv2` to the workspace `Cargo.toml`.
10. Build & fix any linking or include-path issues.
11. **Run the test suite.**  Confirm no regressions in process-graph
    behavior, external-port connections, UI show/hide, and state
    serialize/deserialize.

---

## 9. What this design *intentionally* avoids

- No changes to `CarlaLV2ProcessingChain` itself (it stays untouched).
- No changes to `GraphFXChain`.
- No changes to the existing `FXChain` Rust wrapper or the C API — they
  continue to operate against `ProcessingChainInterface` pointers which
  now happen to point to adapter instances.
- No attempt to pass `shoop_shared_ptr` through cxx (raw pointers with
  no-op deleters are used instead, which is safe in this single-binary
  context).
- The inner bridge cxx module and the outer bridge module are separate
  files but compiled together in one `carla_lv2_cxx` library by calling
  `cxx_build::bridges(&[...])` from a single `build.rs`.

---

## 10. Future: removing the inner bridge

When the processing chain is ported to pure Rust:

1. Rip out `CarlaLv2ChainCppFacade` and the inner bridge module.
2. Rewrite `FxChain` in Rust — it will own LV2 plugin handles directly
   via Rust LV2 libraries.
3. Port management will also move to Rust (the port pointers returned by
   the outer bridge will come from Rust-owned port objects instead of
   C++ owned ones).
4. The outer bridge's C++ interface stays *identical* — no changes to
   `RustFxChainAdapter` or `BackendSession`.
5. The LV2StateString logic (mutex, JSON, base64, wait-for-ready) can
   be ported one piece at a time while the outer bridge holds the
   integration together.
