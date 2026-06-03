#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <utility>

class DecoupledMidiPort;

template<typename T>
class BridgeWeak;

template<typename T>
class BridgeStrong {
public:
    BridgeStrong() = default;
    explicit BridgeStrong(std::shared_ptr<T> ptr) : m_ptr(std::move(ptr)) {}

    BridgeWeak<T> downgrade() const { return BridgeWeak<T>(m_ptr); }
    std::unique_ptr<BridgeWeak<T>> downgrade_unique() const { return std::make_unique<BridgeWeak<T>>(m_ptr); }
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
    std::unique_ptr<BridgeWeak<T>> clone_unique() const { return std::make_unique<BridgeWeak<T>>(m_ptr); }

private:
    std::weak_ptr<T> m_ptr;
};

#define SHOOP_DECLARE_TYPED_BRIDGE_OBJECT(CppType, StrongType, WeakType, func_prefix, method_suffix) \
using WeakType = BridgeWeak<CppType>; \
using StrongType = BridgeStrong<CppType>; \
std::unique_ptr<StrongType> make_##func_prefix##_strong(std::shared_ptr<CppType> p); \
std::shared_ptr<CppType> func_prefix##_lock(const WeakType &weak);

#define SHOOP_DEFINE_TYPED_BRIDGE_OBJECT(CppType, StrongType, WeakType, func_prefix, method_suffix) \
std::unique_ptr<StrongType> make_##func_prefix##_strong(std::shared_ptr<CppType> p) { \
    return std::make_unique<StrongType>(std::move(p)); \
} \
std::shared_ptr<CppType> func_prefix##_lock(const WeakType &weak) { \
    auto strong = weak.upgrade(); \
    return strong ? strong->shared_ptr() : nullptr; \
}

