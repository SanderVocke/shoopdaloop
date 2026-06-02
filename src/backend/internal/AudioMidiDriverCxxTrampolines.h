#pragma once

#include <cstdint>

#include "AudioMidiDriver.h"
#include "BridgeObject.h"
#include "DecoupledMidiPort.h"

namespace backend_rust {
class CommandQueue;
}

class HasAudioProcessingFunction;
class DecoupledMidiPort;

namespace backend_rust {

void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr);
void audiomididriver_exec_command_queue(uintptr_t command_queue_ptr);


}
