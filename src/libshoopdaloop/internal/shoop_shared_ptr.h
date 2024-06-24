#pragma once

#ifdef USE_TRACKED_SHARED_PTRS
#include "tracked_shared_ptr.h"
template<typename T>
using shoop_shared_ptr = tracked_shared_ptr<T>;
template<typename T>
using shoop_weak_ptr = tracked_weak_ptr<T>;
template<typename T>
using shoop_enable_shared_from_this = tracked_enable_shared_from_this<T>;

template<typename T, typename ...Args>
shoop_shared_ptr<T> shoop_make_shared(Args&&... args) { return make_tracked_shared<T>(std::forward<Args>(args)...); }

template<class To, class From>
shoop_shared_ptr<To> shoop_dynamic_pointer_cast(shoop_shared_ptr<From> const& other) { return dynamic_tracked_pointer_cast<To>(other); }
template<class To, class From>
shoop_shared_ptr<To> shoop_static_pointer_cast(shoop_shared_ptr<From> const& other) { return static_tracked_pointer_cast<To>(other); }

#else

#include <memory>
template<typename T>
using shoop_shared_ptr = std::shared_ptr<T>;
template<typename T>
using shoop_weak_ptr = std::weak_ptr<T>;
template<typename T>
using shoop_enable_shared_from_this = std::enable_shared_from_this<T>;

template<typename T, typename ...Args>
shoop_shared_ptr<T> shoop_make_shared(Args&&... args) { return std::make_shared<T>(std::forward<Args>(args)...); }

template<class To, class From>
shoop_shared_ptr<To> shoop_dynamic_pointer_cast(shoop_shared_ptr<From> const& other) { return std::dynamic_pointer_cast<To>(other); }
template<class To, class From>
shoop_shared_ptr<To> shoop_static_pointer_cast(shoop_shared_ptr<From> const& other) { return std::static_pointer_cast<To>(other); }

#endif