#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <utility>

class HasAudioProcessingFunction;
class DecoupledMidiPort;

namespace bridge_object {

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

#define SHOOP_DECLARE_TYPED_BRIDGE_OBJECT(CppType, StrongType, WeakType, func_prefix, method_suffix) \
class WeakType; \
class StrongType final : public BridgeStrong<CppType> { \
public: \
    using BridgeStrong<CppType>::BridgeStrong; \
    std::unique_ptr<WeakType> downgrade_##method_suffix() const; \
}; \
class WeakType final : public BridgeWeak<CppType> { \
public: \
    using BridgeWeak<CppType>::BridgeWeak; \
    std::unique_ptr<StrongType> upgrade_##method_suffix() const; \
}; \
std::unique_ptr<StrongType> make_##func_prefix##_strong(std::shared_ptr<CppType> p); \
std::unique_ptr<WeakType> func_prefix##_downgrade(const StrongType &strong); \
std::unique_ptr<StrongType> func_prefix##_upgrade(const WeakType &weak); \
std::unique_ptr<WeakType> func_prefix##_clone_weak(const WeakType &weak); \
std::shared_ptr<CppType> func_prefix##_lock(const WeakType &weak);

SHOOP_DECLARE_TYPED_BRIDGE_OBJECT(HasAudioProcessingFunction, ProcessorBridgeStrong, ProcessorBridgeWeak, processor_bridge, processor)
void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes);

SHOOP_DECLARE_TYPED_BRIDGE_OBJECT(DecoupledMidiPort, DecoupledMidiPortBridgeStrong, DecoupledMidiPortBridgeWeak, decoupled_midi_port_bridge, decoupled_midi_port)
void decoupled_midi_port_bridge_proc_process(const DecoupledMidiPortBridgeWeak &weak, uint32_t nframes);
void decoupled_midi_port_bridge_close(const DecoupledMidiPortBridgeWeak &weak);

#undef SHOOP_DECLARE_TYPED_BRIDGE_OBJECT

}
