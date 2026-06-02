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

class DecoupledMidiPortBridgeWeak;

class DecoupledMidiPortBridgeStrong final : public BridgeStrong<DecoupledMidiPort> {
public:
    using BridgeStrong<DecoupledMidiPort>::BridgeStrong;
    std::unique_ptr<DecoupledMidiPortBridgeWeak> downgrade_decoupled_midi_port() const;
};

class DecoupledMidiPortBridgeWeak final : public BridgeWeak<DecoupledMidiPort> {
public:
    using BridgeWeak<DecoupledMidiPort>::BridgeWeak;
    std::unique_ptr<DecoupledMidiPortBridgeStrong> upgrade_decoupled_midi_port() const;
};

std::unique_ptr<DecoupledMidiPortBridgeStrong> make_decoupled_midi_port_bridge_strong(std::shared_ptr<DecoupledMidiPort> p);
std::unique_ptr<DecoupledMidiPortBridgeWeak> decoupled_midi_port_bridge_downgrade(const DecoupledMidiPortBridgeStrong &strong);
std::unique_ptr<DecoupledMidiPortBridgeStrong> decoupled_midi_port_bridge_upgrade(const DecoupledMidiPortBridgeWeak &weak);
std::unique_ptr<DecoupledMidiPortBridgeWeak> decoupled_midi_port_bridge_clone_weak(const DecoupledMidiPortBridgeWeak &weak);
std::shared_ptr<DecoupledMidiPort> decoupled_midi_port_bridge_lock(const DecoupledMidiPortBridgeWeak &weak);
void decoupled_midi_port_bridge_proc_process(const DecoupledMidiPortBridgeWeak &weak, uint32_t nframes);
void decoupled_midi_port_bridge_close(const DecoupledMidiPortBridgeWeak &weak);

}
