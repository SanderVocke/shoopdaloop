#pragma once 
#include <cstddef>
#include <memory>

template<typename T>
class tracked_shared_ptr {
public:
    using StdPtr = std::shared_ptr<T>;
    StdPtr m_ptr;

    constexpr tracked_shared_ptr() : m_ptr(StdPtr()) {}
    constexpr tracked_shared_ptr(std::nullptr_t) : m_ptr(StdPtr(std::nullptr_t{})) {}
    
    template<typename First>
    explicit tracked_shared_ptr(First first) : m_ptr(StdPtr(first)) {}

    template<typename First, typename Second>
    explicit tracked_shared_ptr(First first, Second second) : m_ptr(StdPtr(first, second)) {}

    template<typename First, typename Second, typename Third>
    explicit tracked_shared_ptr(First first, Second second, Third third) : m_ptr(StdPtr(first, second, third)) {}

    ~tracked_shared_ptr() noexcept { m_ptr = nullptr; }

    tracked_shared_ptr &operator=(std::nullptr_t) noexcept { m_ptr = nullptr; return *this; }
    tracked_shared_ptr &operator=(tracked_shared_ptr<T> const& rhs) noexcept { m_ptr = rhs.m_ptr; return *this; }
    tracked_shared_ptr &operator=(std::shared_ptr<T> const& rhs) noexcept { m_ptr = rhs.m_ptr; return *this; }

    template<typename First>
    void reset(First first) { m_ptr.reset(first); }

    template<typename First, typename Second>
    void reset(First first, Second second) { m_ptr.reset(first, second); }

    T* get() const noexcept { return m_ptr.get(); }

    T& operator*() const noexcept { return *m_ptr; }

    explicit operator bool() const noexcept { return (bool)m_ptr; }

    T* operator->() const noexcept { return get(); }
};

template<typename T>
class tracked_weak_ptr {
public:
    using StdPtr = std::weak_ptr<T>;
    StdPtr m_ptr;

    constexpr tracked_weak_ptr() noexcept : m_ptr(StdPtr()) {}

    template <typename _T>
    tracked_weak_ptr(tracked_shared_ptr<_T> const&p) noexcept : m_ptr(StdPtr(p.m_ptr)) {}

    template <typename _T>
    tracked_weak_ptr(tracked_shared_ptr<_T> &&p) noexcept : m_ptr(StdPtr(p.m_ptr)) {}

    template <typename _T>
    tracked_weak_ptr(std::weak_ptr<_T> &&p) noexcept : m_ptr(p) {}

    template <typename First, typename Second>
    tracked_weak_ptr(First first, Second second) noexcept : m_ptr(StdPtr(first, second)) {}

    tracked_shared_ptr<T> lock() const noexcept { return tracked_shared_ptr<T>(m_ptr.lock()); }

    template<typename... Args>
    void reset(Args&&... args) { m_ptr.reset(std::forward<Args>(args)...); }
};

namespace std {

template<class T>
struct is_pointer<tracked_shared_ptr<T>> : std::true_type {};

template<typename T>
struct owner_less<tracked_shared_ptr<T>> {
    using Std = std::owner_less<std::shared_ptr<T>>;
    Std m_std;
    owner_less() : m_std(Std()) {}

    bool operator()(const tracked_shared_ptr<T> &lhs, const tracked_shared_ptr<T> &rhs) const {
        return m_std.operator()(lhs.m_ptr, rhs.m_ptr);
    }

    bool operator()(const tracked_shared_ptr<T> &lhs, const tracked_weak_ptr<T> &rhs) const {
        return m_std.operator()(lhs.m_ptr, rhs.m_ptr);
    }

    bool operator()(const tracked_weak_ptr<T> &lhs, const tracked_weak_ptr<T> &rhs) const {
        return m_std.operator()(lhs.m_ptr, rhs.m_ptr);
    }
};

template<typename T>
struct owner_less<tracked_weak_ptr<T>> {
    using Std = std::owner_less<std::weak_ptr<T>>;
    Std m_std;
    owner_less() : m_std(Std()) {}

    bool operator()(const tracked_weak_ptr<T> &lhs, const tracked_weak_ptr<T> &rhs) const {
        return m_std.operator()(lhs.m_ptr, rhs.m_ptr);
    }

    bool operator()(const tracked_shared_ptr<T> &lhs, const tracked_weak_ptr<T> &rhs) const {
        return m_std.operator()(lhs.m_ptr, rhs.m_ptr);
    }

    bool operator()(const tracked_weak_ptr<T> &lhs, const tracked_shared_ptr<T> &rhs) const {
        return m_std.operator()(lhs.m_ptr, rhs.m_ptr);
    }
};

} // namespace std

template<class T1, class T2>
bool operator<(const tracked_shared_ptr<T1>& lhs,
                          const tracked_shared_ptr<T2>& rhs)
{ return lhs.m_ptr < rhs.m_ptr; }

template<class T1, class T2>
bool operator==(const tracked_shared_ptr<T1>& lhs,
                          const tracked_shared_ptr<T2>& rhs)
{ return lhs.m_ptr == rhs.m_ptr; }

template<class T1>
bool operator==(const tracked_shared_ptr<T1>& lhs,
                          std::nullptr_t)
{ return lhs.m_ptr == nullptr; }

template<class T2>
bool operator==(std::nullptr_t,
                          const tracked_shared_ptr<T2>& rhs)
{ return rhs.m_ptr == nullptr; }

template<class To, class From>
tracked_shared_ptr<To> dynamic_tracked_pointer_cast(
    const tracked_shared_ptr<From> &other) noexcept
{
    return tracked_shared_ptr<To>(std::dynamic_pointer_cast<To>(other.m_ptr));
}

template<class To, class From>
tracked_shared_ptr<To> static_tracked_pointer_cast(
    const tracked_shared_ptr<From> &other) noexcept
{
    return tracked_shared_ptr<To>(std::static_pointer_cast<To>(other.m_ptr));
}

template<typename T, typename... Args>
tracked_shared_ptr<T> make_tracked_shared(Args&&... args) { return tracked_shared_ptr<T>(std::make_shared<T>(std::forward<Args>(args)...)); }

template<typename T>
class tracked_enable_shared_from_this : public std::enable_shared_from_this<T> {
    using Std = std::enable_shared_from_this<T>;
    using StdWeak = std::weak_ptr<T>;
    using StdShared = std::shared_ptr<T>;
    using TrackedWeak = tracked_weak_ptr<T>;
    using TrackedShared = tracked_shared_ptr<T>;
public:
    TrackedWeak weak_from_this() { return TrackedWeak(Std::weak_from_this()); }
    TrackedShared shared_from_this() { return TrackedShared(Std::shared_from_this()); }
};