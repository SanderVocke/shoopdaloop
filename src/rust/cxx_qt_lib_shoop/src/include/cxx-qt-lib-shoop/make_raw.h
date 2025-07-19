#pragma once
#include <utility>
template<typename T>
inline T* make_raw() {
    return new T();
}

template<typename T, typename Arg>
inline T* make_raw_with_one_arg(Arg arg) {
    return new T(arg);
}

template<typename T, typename Arg1, typename Arg2>
inline T* make_raw_with_two_args(Arg1 arg1, Arg2 arg2) {
    return new T(arg1, arg2);
}

template<typename T>
void delete_raw(T* ptr) {
    delete ptr;
}