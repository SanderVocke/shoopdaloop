#include "BridgeObject.h"

#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"

namespace bridge_object {
namespace {
struct HolderBase {
    virtual ~HolderBase() = default;
    virtual uint32_t type_id() const = 0;
};

template<typename T, BridgeObjectType K>
struct Holder final : HolderBase {
    shoop_shared_ptr<T> ptr;
    explicit Holder(shoop_shared_ptr<T> p) : ptr(std::move(p)) {}
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
BridgeStrongHandle reg(shoop_shared_ptr<T> p) {
    auto &r = registry();
    auto h = std::make_shared<Holder<T, K>>(std::move(p));
    std::lock_guard<std::mutex> lk(r.mtx);
    auto id = r.next_id++;
    r.strongs[id] = h;
    r.weaks[id] = h;
    return BridgeStrongHandle{id, static_cast<uint32_t>(K)};
}

template<typename T, BridgeObjectType K>
std::optional<shoop_shared_ptr<T>> lock_typed(BridgeWeakHandle weak) {
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

BridgeStrongHandle register_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    return reg<HasAudioProcessingFunction, BridgeObjectType::Processor>(std::move(p));
}

BridgeStrongHandle register_decoupled_midi_port(shoop_shared_ptr<DecoupledMidiPort> p) {
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

std::optional<shoop_shared_ptr<HasAudioProcessingFunction>> lock_processor(BridgeWeakHandle weak) {
    return lock_typed<HasAudioProcessingFunction, BridgeObjectType::Processor>(weak);
}

std::optional<shoop_shared_ptr<DecoupledMidiPort>> lock_decoupled_midi_port(BridgeWeakHandle weak) {
    return lock_typed<DecoupledMidiPort, BridgeObjectType::DecoupledMidiPort>(weak);
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

}
