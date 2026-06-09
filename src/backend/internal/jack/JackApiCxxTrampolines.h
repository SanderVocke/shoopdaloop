#pragma once

#include <cstdint>

namespace rust {
inline namespace cxxbridge1 {
class Str;
}
}

namespace backend_rust {
int jackapi_process_cb(uintptr_t driver_ptr, uint32_t nframes);
int jackapi_xrun_cb(uintptr_t driver_ptr);
void jackapi_port_update_cb(uintptr_t driver_ptr);
void jackapi_error_log(rust::Str msg);
void jackapi_info_log(rust::Str msg);
}
