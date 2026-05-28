#pragma once

#include <cstddef>

namespace backend_rust {
void cxx_command_execute_ptr(std::size_t user_data) noexcept;
void cxx_command_destroy_ptr(std::size_t user_data) noexcept;
}
