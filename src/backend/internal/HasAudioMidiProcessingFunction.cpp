#include "HasAudioProcessingFunction.h"

void processor_bridge_proc_process(const ProcessorBridgeWeak &weak, uint32_t nframes) {
    auto processor = weak.lock();
    if (processor) { processor->PROC_process(nframes); }
}