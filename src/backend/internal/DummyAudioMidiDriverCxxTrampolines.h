#pragma once
#include <cstdint>

namespace backend_rust {
void dummy_audiomididriver_exec_commands(uintptr_t owner_ptr);
void dummy_audiomididriver_process(uintptr_t owner_ptr, uint32_t nframes);
}
