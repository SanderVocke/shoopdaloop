#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <unordered_map>
#include <mutex>

#include <memory>
#include <utility>

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

template<typename T>
class BridgeWeak;

template<typename T>
class BridgeStrong {
public:
    BridgeStrong() = default;
    explicit BridgeStrong(std::shared_ptr<T> ptr) : m_ptr(std::move(ptr)) {}

    BridgeWeak<T> downgrade() const { return BridgeWeak<T>(m_ptr); }
    std::shared_ptr<T> shared_ptr() const { return m_ptr; }
    T* ptr() const { return m_ptr.get(); }
    bool valid() const { return static_cast<bool>(m_ptr); }

private:
    std::shared_ptr<T> m_ptr;
};

template<typename T>
class BridgeWeak {
public:
    BridgeWeak() = default;
    explicit BridgeWeak(std::weak_ptr<T> ptr) : m_ptr(std::move(ptr)) {}

    std::unique_ptr<BridgeStrong<T>> upgrade() const {
        auto ptr = m_ptr.lock();
        if (!ptr) { return {}; }
        return std::make_unique<BridgeStrong<T>>(std::move(ptr));
    }

private:
    std::weak_ptr<T> m_ptr;
};

class ProcessorBridgeWeak;

class ProcessorBridgeStrong final : public BridgeStrong<HasAudioProcessingFunction> {
public:
    using BridgeStrong<HasAudioProcessingFunction>::BridgeStrong;
    std::unique_ptr<ProcessorBridgeWeak> downgrade_processor() const;
};

class ProcessorBridgeWeak final : public BridgeWeak<HasAudioProcessingFunction> {
public:
    using BridgeWeak<HasAudioProcessingFunction>::BridgeWeak;
    std::unique_ptr<ProcessorBridgeStrong> upgrade_processor() const;
};

std::unique_ptr<ProcessorBridgeStrong> make_processor_bridge_strong(std::shared_ptr<HasAudioProcessingFunction> p);
std::unique_ptr<ProcessorBridgeWeak> processor_bridge_downgrade(const ProcessorBridgeStrong &strong);
std::unique_ptr<ProcessorBridgeStrong> processor_bridge_upgrade(const ProcessorBridgeWeak &weak);
std::unique_ptr<ProcessorBridgeWeak> processor_bridge_clone_weak(const ProcessorBridgeWeak &weak);
std::shared_ptr<HasAudioProcessingFunction> processor_bridge_lock(const ProcessorBridgeWeak &weak);
void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes);

BridgeStrongHandle register_processor(std::shared_ptr<HasAudioProcessingFunction> p);
BridgeStrongHandle register_decoupled_midi_port(std::shared_ptr<DecoupledMidiPort> p);

BridgeWeakHandle downgrade(BridgeStrongHandle strong);
BridgeStrongHandle clone_strong(BridgeStrongHandle strong);
void release_strong(BridgeStrongHandle strong);

std::optional<std::shared_ptr<HasAudioProcessingFunction>> lock_processor(BridgeWeakHandle weak);
std::optional<std::shared_ptr<DecoupledMidiPort>> lock_decoupled_midi_port(BridgeWeakHandle weak);

// Typed resolvers for CXX/Rust callers. Return empty shared_ptr on stale/type mismatch.
std::shared_ptr<HasAudioProcessingFunction> bridge_resolve_processor_for_rust(uint64_t weak_id, uint32_t weak_type_id);
std::shared_ptr<DecoupledMidiPort> bridge_resolve_decoupled_midi_port_for_rust(uint64_t weak_id, uint32_t weak_type_id);

// Typed CXX operation helpers for resolved shared_ptrs. These deliberately take
// typed shared_ptrs, not bridge handle IDs, so callers demonstrate the resolver
// pattern and avoid handle-based trampolines.
void bridge_processor_proc_process(std::shared_ptr<HasAudioProcessingFunction> processor, uint32_t nframes);
void bridge_decoupled_midi_port_proc_process(std::shared_ptr<DecoupledMidiPort> port, uint32_t nframes);
void bridge_decoupled_midi_port_close(std::shared_ptr<DecoupledMidiPort> port);

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
