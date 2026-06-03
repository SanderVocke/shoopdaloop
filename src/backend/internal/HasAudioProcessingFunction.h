#pragma once

#include <cstdint>
#include "BridgeObject.h"

class HasAudioProcessingFunction {
public:
    HasAudioProcessingFunction() {}
    virtual ~HasAudioProcessingFunction() {}

    virtual void PROC_process(uint32_t nframes) = 0;
};

using ProcessorBridgeWeak = BridgeWeak<HasAudioProcessingFunction>;
using ProcessorBridgeStrong = BridgeStrong<HasAudioProcessingFunction>;
void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes);