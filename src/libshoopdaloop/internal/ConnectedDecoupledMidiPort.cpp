#include "ConnectedDecoupledMidiPort.h"

ConnectedDecoupledMidiPort::ConnectedDecoupledMidiPort(std::shared_ptr<_DecoupledMidiPort> port, std::shared_ptr<BackendSession> backend) :
    port(port), backend(backend) {}

BackendSession &ConnectedDecoupledMidiPort::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}