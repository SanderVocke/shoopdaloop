#pragma once

#include <memory>

template<typename T>
inline std::unique_ptr<T> make_unique() {
    return std::make_unique<T>();
}