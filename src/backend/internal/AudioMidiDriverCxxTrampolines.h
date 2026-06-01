#pragma once

#include <cstdint>

namespace backend_rust {
class CommandQueue;
}

class HasAudioProcessingFunction;
class DecoupledMidiPort;

namespace backend_rust {

void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr);
void audiomididriver_exec_command_queue(uintptr_t command_queue_ptr);
void audiomididriver_process_processor(uint64_t processor_id, uint32_t processor_type_id, uint32_t nframes);
void audiomididriver_process_decoupled_port(uint64_t decoupled_port_id, uint32_t decoupled_port_type_id, uint32_t nframes);
void audiomididriver_close_decoupled_port(uint64_t decoupled_port_id, uint32_t decoupled_port_type_id);

}
