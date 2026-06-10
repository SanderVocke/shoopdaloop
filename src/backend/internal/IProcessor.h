#pragma once

#include <cstdint>
#include "BridgeObject.h"

class IProcessor {
public:
    IProcessor() {}
    virtual ~IProcessor() {}

    virtual void PROC_process(uint32_t nframes) = 0;
};

using ProcessorBridgeWeak = BridgeWeak<IProcessor>;
using ProcessorBridgeStrong = BridgeStrong<IProcessor>;
