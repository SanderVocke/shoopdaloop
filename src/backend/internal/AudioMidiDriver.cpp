#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include "BridgeObject.h"
#include "RustCommandQueue.h"
#include <cstdint>
#include "AudioMidiDriverCxxTrampolines.h"

namespace backend_rust {
void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr) {
    if (!maybe_fn_ptr) { return; }
    auto fn = reinterpret_cast<void (*)()>(maybe_fn_ptr);
    fn();
}

void audiomididriver_exec_command_queue(uintptr_t command_queue_ptr) {
    if (!command_queue_ptr) { return; }
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(command_queue_ptr);
    rust_command_queue::exec_all(*queue);
}

void audiomididriver_process_processor(uint64_t processor_id, uint32_t processor_type_id, uint32_t nframes) {
    auto maybe = bridge_object::lock_processor(bridge_object::BridgeWeakHandle{processor_id, processor_type_id});
    if (!maybe) { return; }
    (*maybe)->PROC_process(nframes);
}

void audiomididriver_process_decoupled_port(uintptr_t decoupled_port_ptr, uint32_t nframes) {
    if (!decoupled_port_ptr) { return; }
    auto *port = reinterpret_cast<::DecoupledMidiPort *>(decoupled_port_ptr);
    port->PROC_process(nframes);
}

void audiomididriver_close_decoupled_port(uintptr_t decoupled_port_ptr) {
    if (!decoupled_port_ptr) { return; }
    auto *port = reinterpret_cast<::DecoupledMidiPort *>(decoupled_port_ptr);
    port->close();
}
}
