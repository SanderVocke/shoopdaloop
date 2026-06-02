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


}
