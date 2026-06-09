#pragma once

#include <cstdint>

#include "BridgeObject.h"
#include "IProcessor.h"

namespace backend_rust {
class CommandQueue;
}

namespace backend_rust {
void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr);
}
