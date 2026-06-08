#pragma once

#include <cstdint>

#include "BridgeObject.h"
#include "IProcessor.h"

namespace backend_rust {
class CommandQueue;
}

namespace backend_rust {
void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr);
void audiomididriver_request_close_decoupled_midi_port(uintptr_t driver_ptr, uint64_t registry_handle);
}