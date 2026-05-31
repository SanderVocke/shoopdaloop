#include "CommandToken.h"

#include <exception>

namespace backend_rust {
std::size_t make_command_token_ptr(std::function<void()> fn) {
    return reinterpret_cast<std::size_t>(new std::function<void()>(std::move(fn)));
}

void cxx_command_execute_ptr(std::size_t user_data) noexcept {
    auto fn = reinterpret_cast<std::function<void()>*>(user_data);
    if (!fn) return;
    try {
        (*fn)();
    } catch (...) {
        delete fn;
        std::terminate();
    }
    delete fn;
}

void cxx_command_destroy_ptr(std::size_t user_data) noexcept {
    auto fn = reinterpret_cast<std::function<void()>*>(user_data);
    delete fn;
}
}
