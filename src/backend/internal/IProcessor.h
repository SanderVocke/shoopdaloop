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

inline void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes) {
    auto processor = weak.lock();
    if (processor) { processor->PROC_process(nframes); }
}