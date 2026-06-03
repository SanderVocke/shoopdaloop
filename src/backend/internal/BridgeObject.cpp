#include "BridgeObject.h"

#include "DecoupledMidiPort.h"
#include "HasAudioProcessingFunction.h"

namespace bridge_object {

#define SHOOP_DEFINE_TYPED_BRIDGE_OBJECT(CppType, StrongType, WeakType, func_prefix, method_suffix) \
std::unique_ptr<WeakType> StrongType::downgrade_##method_suffix() const { \
    return std::make_unique<WeakType>(shared_ptr()); \
} \
std::unique_ptr<StrongType> WeakType::upgrade_##method_suffix() const { \
    auto strong = BridgeWeak<CppType>::upgrade(); \
    if (!strong) { return {}; } \
    return std::make_unique<StrongType>(strong->shared_ptr()); \
} \
std::unique_ptr<StrongType> make_##func_prefix##_strong(std::shared_ptr<CppType> p) { \
    return std::make_unique<StrongType>(std::move(p)); \
} \
std::unique_ptr<WeakType> func_prefix##_downgrade(const StrongType &strong) { \
    return strong.downgrade_##method_suffix(); \
} \
std::unique_ptr<StrongType> func_prefix##_upgrade(const WeakType &weak) { \
    return weak.upgrade_##method_suffix(); \
} \
std::unique_ptr<WeakType> func_prefix##_clone_weak(const WeakType &weak) { \
    auto strong = weak.upgrade_##method_suffix(); \
    if (!strong) { return {}; } \
    return strong->downgrade_##method_suffix(); \
} \
std::shared_ptr<CppType> func_prefix##_lock(const WeakType &weak) { \
    auto strong = weak.upgrade_##method_suffix(); \
    return strong ? strong->shared_ptr() : nullptr; \
}

SHOOP_DEFINE_TYPED_BRIDGE_OBJECT(HasAudioProcessingFunction, ProcessorBridgeStrong, ProcessorBridgeWeak, processor_bridge, processor)
SHOOP_DEFINE_TYPED_BRIDGE_OBJECT(DecoupledMidiPort, DecoupledMidiPortBridgeStrong, DecoupledMidiPortBridgeWeak, decoupled_midi_port_bridge, decoupled_midi_port)

#undef SHOOP_DEFINE_TYPED_BRIDGE_OBJECT

void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes) {
    auto processor = processor_bridge_lock(weak);
    if (processor) { processor->PROC_process(nframes); }
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
