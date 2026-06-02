#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <unordered_map>
#include <mutex>

#include "shoop_shared_ptr.h"

class HasAudioProcessingFunction;
class DecoupledMidiPort;

namespace bridge_object {

// First-iteration registry notes:
// - The bridge registry is protected by a global mutex.
// - Registration / clone / release operations are not real-time safe.
// - Weak-handle lock/resolve currently also takes the mutex briefly, so process-thread
//   use is functionally safe but not RT-hard-safe yet.
// - This is acceptable for the current migration milestone and should be replaced by
//   a process-thread-safe policy in a follow-up iteration.

enum class BridgeObjectType : uint32_t {
    Invalid = 0,
    Processor = 1,
    DecoupledMidiPort = 2,
};

struct BridgeStrongHandle {
    uint64_t id = 0;
    uint32_t type_id = 0;
};

struct BridgeWeakHandle {
    uint64_t id = 0;
    uint32_t type_id = 0;
};

BridgeStrongHandle register_processor(shoop_shared_ptr<HasAudioProcessingFunction> p);
BridgeStrongHandle register_decoupled_midi_port(shoop_shared_ptr<DecoupledMidiPort> p);

BridgeWeakHandle downgrade(BridgeStrongHandle strong);
BridgeStrongHandle clone_strong(BridgeStrongHandle strong);
void release_strong(BridgeStrongHandle strong);

std::optional<shoop_shared_ptr<HasAudioProcessingFunction>> lock_processor(BridgeWeakHandle weak);
std::optional<shoop_shared_ptr<DecoupledMidiPort>> lock_decoupled_midi_port(BridgeWeakHandle weak);

}

namespace backend_rust {

// Scalar shims for Rust/CXX. They operate on the C++ bridge registry while
// avoiding CXX type-identity issues between backend_rust::* generated handle
// structs and bridge_object::* native handle structs.
bool bridge_upgrade_for_rust(uint64_t weak_id, uint32_t weak_type_id);
void bridge_release_strong_for_rust(uint64_t strong_id, uint32_t strong_type_id);

// Generic scalar shims for C++ callers. These route Rust-owned IDs back into
// the Rust registry and C++-owned IDs into the C++ registry.
bool bridge_upgrade_generic(uint64_t weak_id, uint32_t weak_type_id);
void bridge_release_strong_generic(uint64_t strong_id, uint32_t strong_type_id);

}
