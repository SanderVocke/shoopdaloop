#pragma once
#include <memory>
#include "shoop_globals.h"

class Backend;

using namespace shoop_types;

class ConnectedDecoupledMidiPort : public std::enable_shared_from_this<ConnectedDecoupledMidiPort> {
public:
    const std::shared_ptr<shoop_types::_DecoupledMidiPort> port;
    const std::weak_ptr<Backend> backend;

    ConnectedDecoupledMidiPort(std::shared_ptr<_DecoupledMidiPort> port, std::shared_ptr<Backend> backend);

    Backend &get_backend();
};