#include "BridgeObject.h"

#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"

namespace bridge_object {

std::unique_ptr<ProcessorBridgeWeak> ProcessorBridgeStrong::downgrade_processor() const {
    return std::make_unique<ProcessorBridgeWeak>(shared_ptr());
}

std::unique_ptr<ProcessorBridgeStrong> ProcessorBridgeWeak::upgrade_processor() const {
    auto strong = BridgeWeak<HasAudioProcessingFunction>::upgrade();
    if (!strong) { return {}; }
    return std::make_unique<ProcessorBridgeStrong>(strong->shared_ptr());
}

std::unique_ptr<ProcessorBridgeStrong> make_processor_bridge_strong(std::shared_ptr<HasAudioProcessingFunction> p) {
    return std::make_unique<ProcessorBridgeStrong>(std::move(p));
}

std::unique_ptr<ProcessorBridgeWeak> processor_bridge_downgrade(const ProcessorBridgeStrong &strong) {
    return strong.downgrade_processor();
}

std::unique_ptr<ProcessorBridgeStrong> processor_bridge_upgrade(const ProcessorBridgeWeak &weak) {
    return weak.upgrade_processor();
}

std::unique_ptr<ProcessorBridgeWeak> processor_bridge_clone_weak(const ProcessorBridgeWeak &weak) {
    auto strong = weak.upgrade_processor();
    if (!strong) { return {}; }
    return strong->downgrade_processor();
}

std::shared_ptr<HasAudioProcessingFunction> processor_bridge_lock(const ProcessorBridgeWeak &weak) {
    auto strong = weak.upgrade_processor();
    return strong ? strong->shared_ptr() : nullptr;
}

void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes) {
    auto processor = processor_bridge_lock(weak);
    if (processor) { processor->PROC_process(nframes); }
}

std::unique_ptr<DecoupledMidiPortBridgeWeak> DecoupledMidiPortBridgeStrong::downgrade_decoupled_midi_port() const {
    return std::make_unique<DecoupledMidiPortBridgeWeak>(shared_ptr());
}

std::unique_ptr<DecoupledMidiPortBridgeStrong> DecoupledMidiPortBridgeWeak::upgrade_decoupled_midi_port() const {
    auto strong = BridgeWeak<DecoupledMidiPort>::upgrade();
    if (!strong) { return {}; }
    return std::make_unique<DecoupledMidiPortBridgeStrong>(strong->shared_ptr());
}

std::unique_ptr<DecoupledMidiPortBridgeStrong> make_decoupled_midi_port_bridge_strong(std::shared_ptr<DecoupledMidiPort> p) {
    return std::make_unique<DecoupledMidiPortBridgeStrong>(std::move(p));
}

std::unique_ptr<DecoupledMidiPortBridgeWeak> decoupled_midi_port_bridge_downgrade(const DecoupledMidiPortBridgeStrong &strong) {
    return strong.downgrade_decoupled_midi_port();
}

std::unique_ptr<DecoupledMidiPortBridgeStrong> decoupled_midi_port_bridge_upgrade(const DecoupledMidiPortBridgeWeak &weak) {
    return weak.upgrade_decoupled_midi_port();
}

std::unique_ptr<DecoupledMidiPortBridgeWeak> decoupled_midi_port_bridge_clone_weak(const DecoupledMidiPortBridgeWeak &weak) {
    auto strong = weak.upgrade_decoupled_midi_port();
    if (!strong) { return {}; }
    return strong->downgrade_decoupled_midi_port();
}

std::shared_ptr<DecoupledMidiPort> decoupled_midi_port_bridge_lock(const DecoupledMidiPortBridgeWeak &weak) {
    auto strong = weak.upgrade_decoupled_midi_port();
    return strong ? strong->shared_ptr() : nullptr;
}

void decoupled_midi_port_bridge_proc_process(const DecoupledMidiPortBridgeWeak &weak, uint32_t nframes) {
    auto port = decoupled_midi_port_bridge_lock(weak);
    if (port) { port->PROC_process(nframes); }
}

void decoupled_midi_port_bridge_close(const DecoupledMidiPortBridgeWeak &weak) {
    auto port = decoupled_midi_port_bridge_lock(weak);
    if (port) { port->close(); }
}

}
