#pragma once

template<typename T>
inline T* make_raw() {
    return new T();
}

template<typename T>
void delete_raw(T* ptr) {
    delete ptr;
}