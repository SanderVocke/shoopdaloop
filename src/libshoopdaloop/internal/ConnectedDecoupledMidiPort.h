#pragma once
#include <memory>
#include "shoop_globals.h"

class BackendSession;

using namespace shoop_types;

class ConnectedDecoupledMidiPort : public std::enable_shared_from_this<ConnectedDecoupledMidiPort> {
public:
    const std::shared_ptr<shoop_types::_DecoupledMidiPort> port;
    const std::weak_ptr<BackendSession> backend;

    ConnectedDecoupledMidiPort(std::shared_ptr<_DecoupledMidiPort> port, std::shared_ptr<BackendSession> backend);

    BackendSession &get_backend();
};