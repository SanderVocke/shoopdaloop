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

    std::unique_ptr<BridgeWeak<T>> downgrade() const { return std::make_unique<BridgeWeak<T>>(m_ptr); }
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

    std::shared_ptr<T> lock() const { return m_ptr.lock(); }

    std::unique_ptr<BridgeStrong<T>> upgrade() const {
        auto ptr = lock();
        if (!ptr) { return {}; }
        return std::make_unique<BridgeStrong<T>>(std::move(ptr));
    }

    std::unique_ptr<BridgeWeak<T>> clone() const { return std::make_unique<BridgeWeak<T>>(m_ptr); }

private:
    std::weak_ptr<T> m_ptr;
};