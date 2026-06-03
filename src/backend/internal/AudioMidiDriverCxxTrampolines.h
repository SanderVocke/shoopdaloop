#pragma once

#include <cstdint>

#include "BridgeObject.h"
#include "DecoupledMidiPort.h"
#include "IProcessor.h"

namespace backend_rust {
class CommandQueue;
}

class DecoupledMidiPort;

namespace backend_rust {
void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr);
}