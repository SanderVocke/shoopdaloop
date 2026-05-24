# Plan: Migrate `get_sample_rate` to Rust

## Goal
Move the `get_sample_rate` C interface function from C++ (`libshoopdaloop_backend.cpp`) to Rust (`backend_api_cxx.rs`) using cxx's `WeakPtr<T>`/`SharedPtr<T>` bindings, eliminating the need for manual `internal_audio_driver` helper functions.

## Current Implementation (C++)

Location: `src/backend/libshoopdaloop_backend.cpp`

```cpp
unsigned get_sample_rate(shoop_audio_driver_t *driver) {
  return api_impl<unsigned, log_level_debug_trace>("get_sample_rate", [&]() -> unsigned {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return 48000; }
    return _driver->get_sample_rate();
  }, 0);
}
```

The opaque handle `shoop_audio_driver_t*` is actually a pointer to `shoop_weak_ptr<AudioMidiDriver>`, which is manually converted to `shoop_shared_ptr<AudioMidiDriver>` via `internal_audio_driver()`.

## Target Implementation (Rust)

The function will be implemented in `src/rust/backend_rust/src/backend_api_cxx.rs` using cxx's `WeakPtr<AudioMidiDriver>` directly.

## Key Files

- `src/backend/libshoopdaloop_backend.cpp` - current C++ implementation, will become a thin wrapper calling Rust
- `src/backend/libshoopdaloop_backend.h` - C interface header (unchanged)
- `src/backend/internal/AudioMidiDriver.h` - C++ class with `get_sample_rate()` method
- `src/rust/backend_rust/src/backend_api_cxx.rs` - Rust implementation target
- `src/backend/types.h` - opaque handle typedefs

## Phase 1: Add C++ Type Declaration to cxx Bridge

Declare `AudioMidiDriver` in the cxx bridge with `extern "C++"`, including the include path and the `get_sample_rate` method.

Changes to `backend_api_cxx.rs`:
- Add `extern "C++"` block
- Include the AudioMidiDriver header
- Declare `type AudioMidiDriver` as opaque C++ type
- Declare `fn get_sample_rate(self: &AudioMidiDriver) -> u32`

Note: cxx requires that opaque C++ types can only be passed by reference or via SharedPtr/WeakPtr. We need to use `WeakPtr<AudioMidiDriver>` as the handle type.

## Phase 2: Change Handle Type in Bridge

Currently, the C interface uses raw pointers (`shoop_audio_driver_t*`) which are reinterpretations of `weak_ptr*`. We need to either:

Option A (Preferred): Keep the C interface using raw pointers, but add a conversion helper in the cxx bridge that takes a raw pointer and produces a `WeakPtr<AudioMidiDriver>`.

Option B: Change the internal representation to pass `WeakPtr<AudioMidiDriver>` directly through the bridge.

We will use Option A to minimize changes to the existing C interface contract:
- Add a helper function `unsafe fn upgrade_driver_handle(ptr: *mut c_void) -> SharedPtr<AudioMidiDriver>`
- This will reinterpret the pointer as a `weak_ptr*` and call `lock()` to get a `shared_ptr`

## Phase 3: Implement `get_sample_rate` in Rust

Add the Rust implementation in `backend_api_cxx.rs`:
- Accept the opaque handle as raw pointer
- Convert to SharedPtr via the helper
- Call `get_sample_rate()` on the C++ object
- Return the result with appropriate default (48000) if null

## Phase 4: Update C++ to Call Rust

Modify `libshoopdaloop_backend.cpp`:
- Replace the current implementation with a call to the Rust function via the cxx bridge
- Keep the same function signature for C interface compatibility

## Phase 5: Build and Test

Build and run tests to verify the migration works correctly.

## Build Instructions

From the shoopdaloop repo root:
```
cargo build
cargo test
```

Final build should include:
```
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
cargo test
```

## Testing

1. Rust unit tests: `cargo test` (tests in backend_api_cxx.rs)
2. C++ tests: The `test_runner` executable is built as a side effect of building the backend crate
3. Application tests: `./target/debug/shoopdaloop_dev.sh --self-test`

## Notes

- The logging wrapper (`api_impl`) in C++ provides error handling and tracing. The Rust implementation should ideally replicate this pattern, but for the initial migration we can keep it simple and add logging later.
- The default return value when driver is null is 48000 (a common sample rate).
- The cxx bridge already has a namespace `backend_rust`, which we'll continue using.
- AudioMidiDriver's `get_sample_rate()` returns `uint32_t` which maps to `u32` in Rust.