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
