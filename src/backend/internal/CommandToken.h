#pragma once

#include <cstddef>
#include <functional>

namespace backend_rust {
std::size_t make_command_token_ptr(std::function<void()> fn);
void cxx_command_execute_ptr(std::size_t user_data) noexcept;
void cxx_command_destroy_ptr(std::size_t user_data) noexcept;
}
