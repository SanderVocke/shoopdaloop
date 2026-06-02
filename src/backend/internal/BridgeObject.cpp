#include "BridgeObject.h"

#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include "backend_rust/src/bridge_object_cxx.rs.h"

namespace bridge_object {
namespace {
struct HolderBase {
    virtual ~HolderBase() = default;
    virtual uint32_t type_id() const = 0;
};

template<typename T, BridgeObjectType K>
struct Holder final : HolderBase {
    std::shared_ptr<T> ptr;
    explicit Holder(std::shared_ptr<T> p) : ptr(std::move(p)) {}
    uint32_t type_id() const override { return static_cast<uint32_t>(K); }
};

struct Registry {
    std::mutex mtx;
    uint64_t next_id = 1;
    std::unordered_map<uint64_t, std::shared_ptr<HolderBase>> strongs;
    std::unordered_map<uint64_t, std::weak_ptr<HolderBase>> weaks;
};

Registry& registry() {
    static Registry r;
    return r;
}

template<typename T, BridgeObjectType K>
BridgeStrongHandle reg(std::shared_ptr<T> p) {
    auto &r = registry();
    auto h = std::make_shared<Holder<T, K>>(std::move(p));
    std::lock_guard<std::mutex> lk(r.mtx);
    auto id = r.next_id++;
    r.strongs[id] = h;
    r.weaks[id] = h;
    return BridgeStrongHandle{id, static_cast<uint32_t>(K)};
}

template<typename T, BridgeObjectType K>
std::optional<std::shared_ptr<T>> lock_typed(BridgeWeakHandle weak) {
    if (weak.id == 0 || weak.type_id != static_cast<uint32_t>(K)) { return std::nullopt; }
    auto &r = registry();
    std::shared_ptr<HolderBase> b;
    {
        std::lock_guard<std::mutex> lk(r.mtx);
        auto it = r.weaks.find(weak.id);
        if (it == r.weaks.end()) { return std::nullopt; }
        b = it->second.lock();
    }
    if (!b || b->type_id() != static_cast<uint32_t>(K)) { return std::nullopt; }
    auto typed = std::dynamic_pointer_cast<Holder<T, K>>(b);
    if (!typed) { return std::nullopt; }
    return typed->ptr;
}
}

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

BridgeStrongHandle register_processor(std::shared_ptr<HasAudioProcessingFunction> p) {
    return reg<HasAudioProcessingFunction, BridgeObjectType::Processor>(std::move(p));
}

BridgeStrongHandle register_decoupled_midi_port(std::shared_ptr<DecoupledMidiPort> p) {
    return reg<DecoupledMidiPort, BridgeObjectType::DecoupledMidiPort>(std::move(p));
}

BridgeWeakHandle downgrade(BridgeStrongHandle strong) {
    return BridgeWeakHandle{strong.id, strong.type_id};
}

BridgeStrongHandle clone_strong(BridgeStrongHandle strong) {
    if (strong.id == 0) { return {}; }
    auto &r = registry();
    std::lock_guard<std::mutex> lk(r.mtx);
    auto wit = r.weaks.find(strong.id);
    if (wit == r.weaks.end()) { return {}; }
    auto sp = wit->second.lock();
    if (!sp || sp->type_id() != strong.type_id) { return {}; }
    r.strongs[strong.id] = sp;
    return strong;
}

void release_strong(BridgeStrongHandle strong) {
    auto &r = registry();
    std::lock_guard<std::mutex> lk(r.mtx);
    r.strongs.erase(strong.id);
}

std::optional<std::shared_ptr<HasAudioProcessingFunction>> lock_processor(BridgeWeakHandle weak) {
    return lock_typed<HasAudioProcessingFunction, BridgeObjectType::Processor>(weak);
}

std::optional<std::shared_ptr<DecoupledMidiPort>> lock_decoupled_midi_port(BridgeWeakHandle weak) {
    return lock_typed<DecoupledMidiPort, BridgeObjectType::DecoupledMidiPort>(weak);
}

std::shared_ptr<HasAudioProcessingFunction> bridge_resolve_processor_for_rust(uint64_t weak_id, uint32_t weak_type_id) {
    auto maybe = lock_processor(BridgeWeakHandle{weak_id, weak_type_id});
    return maybe ? *maybe : nullptr;
}

std::shared_ptr<DecoupledMidiPort> bridge_resolve_decoupled_midi_port_for_rust(uint64_t weak_id, uint32_t weak_type_id) {
    auto maybe = lock_decoupled_midi_port(BridgeWeakHandle{weak_id, weak_type_id});
    return maybe ? *maybe : nullptr;
}

void bridge_processor_proc_process(std::shared_ptr<HasAudioProcessingFunction> processor, uint32_t nframes) {
    if (processor) { processor->PROC_process(nframes); }
}

void bridge_decoupled_midi_port_proc_process(std::shared_ptr<DecoupledMidiPort> port, uint32_t nframes) {
    if (port) { port->PROC_process(nframes); }
}

void bridge_decoupled_midi_port_close(std::shared_ptr<DecoupledMidiPort> port) {
    if (port) { port->close(); }
}

}

namespace backend_rust {

bool bridge_upgrade_for_rust(uint64_t weak_id, uint32_t weak_type_id) {
    auto strong = bridge_object::clone_strong(bridge_object::BridgeStrongHandle{weak_id, weak_type_id});
    return strong.id != 0;
}

void bridge_release_strong_for_rust(uint64_t strong_id, uint32_t strong_type_id) {
    bridge_object::release_strong(bridge_object::BridgeStrongHandle{strong_id, strong_type_id});
}

bool bridge_upgrade_generic(uint64_t weak_id, uint32_t weak_type_id) {
    if (backend_rust::bridge_is_rust_owned(weak_id)) {
        return backend_rust::bridge_rust_upgrade(weak_id, weak_type_id);
    }
    return bridge_upgrade_for_rust(weak_id, weak_type_id);
}

void bridge_release_strong_generic(uint64_t strong_id, uint32_t strong_type_id) {
    if (backend_rust::bridge_is_rust_owned(strong_id)) {
        backend_rust::bridge_rust_release_strong(strong_id, strong_type_id);
        return;
    }
    bridge_release_strong_for_rust(strong_id, strong_type_id);
}

}
