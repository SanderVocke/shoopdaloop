#pragma once

template<typename SomePtr, typename SomeOtherPtr>
inline SomeOtherPtr * cast_ptr(SomePtr *obj) {
    return static_cast<SomeOtherPtr*>(obj);
}

template<typename SomePtr, typename SomeOtherPtr>
inline SomeOtherPtr const& cast_ref(SomePtr const& obj) {
    return static_cast<SomeOtherPtr const&>(obj);
}